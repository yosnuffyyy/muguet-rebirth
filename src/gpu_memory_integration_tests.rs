//! Executes the production allocator, upload, binding and retirement paths on Metal.
use super::*;

fn guest_write(g: &Gpu, bytes: &[u8]) {
    // Fixture owns this inline-payload region exclusively until upload returns.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), g.base.add(1024), bytes.len()); }
}

fn color(frame: u32) -> ([u8; 4], Vec<u8>) {
    let rgba = [(frame * 31 % 256) as u8, (frame * 53 % 256) as u8, (frame * 79 % 256) as u8, 255];
    let bytes = rgba.iter().flat_map(|v| (f32::from(*v) / 255.0).to_le_bytes()).collect();
    (rgba, bytes)
}

fn read_layers(g: &mut Gpu, texture: &Texture, layers: u32) -> Vec<[u8; 4]> {
    let buffer = g.device.create_buffer(&BufferDescriptor { label: None, size: 256 * layers as u64,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut encoder = g.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(TexelCopyTextureInfo { texture, mip_level: 0, origin: Origin3d::ZERO, aspect: TextureAspect::All },
        TexelCopyBufferInfo { buffer: &buffer, layout: TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(1) } },
        Extent3d { width: 1, height: 1, depth_or_array_layers: layers });
    g.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(MapMode::Read, move |result| tx.send(result).unwrap());
    g.device.poll(PollType::Wait { submission_index: None, timeout: Some(Duration::from_secs(30)) }).unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = buffer.slice(..).get_mapped_range().unwrap();
    let out = (0..layers).map(|i| bytes[i as usize * 256..i as usize * 256 + 4].try_into().unwrap()).collect();
    drop(bytes);
    buffer.unmap();
    out
}

#[test]
fn metal_shared_uniform_slots_retirement_and_borrowed_uploads() {
    let mut config = wasmtime::Config::new();
    config.wasm_threads(true).shared_memory(true);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let ty = wasmtime::MemoryTypeBuilder::new().min(4).max(Some(4)).shared(true).build().unwrap();
    let mem = SharedMemory::new(&engine, ty).unwrap();
    let mut g = Gpu::new(mem, 0, 65536, 0, None).expect("real Metal adapter required, no silent skip");
    g.create_buffer(1, 16, 4, 0, 0);
    g.s.cb[1][0] = Cb { buf: 1, first: 0 };
    let text = "struct C { value: vec4<f32> }; @group(0) @binding(0) var<uniform> c: C;
        @vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
            var p = array<vec2f, 3>(vec2f(-1,-1), vec2f(3,-1), vec2f(-1,3)); return vec4f(p[i],0,1);
        }
        @fragment fn main() -> @location(0) vec4f { return c.value; }".to_string();
    let module = g.device.create_shader_module(ShaderModuleDescriptor { label: None, source: ShaderSource::Wgsl(text.as_str().into()) });
    let info = parse_wgsl(&text);
    let mut groups = BTreeMap::new();
    groups.insert(0, info.bindings.clone());
    let program = Program { module: module.clone(), info, consts: None,
        pack: Some(Pack { members: vec![PackMember { binding: 0, size: 16, offset: 0 }], size: 16 }),
        text, flat: vec![], group_bindings: groups, variants: RefCell::new(HashMap::new()), label: "uniform memory regression".into(),
        uid: g.uid(), stable: 1, hash: String::new(), vid: false, iid: false, out_locs: vec![0], in_locs: vec![], clip: false };
    let bgl = g.bgl_for(&program, 1, 0);
    let layout = g.device.create_pipeline_layout(&PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bgl.0)], immediate_size: 0 });
    let pipeline = g.device.create_render_pipeline(&RenderPipelineDescriptor { label: None, layout: Some(&layout),
        vertex: VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
        primitive: Default::default(), depth_stencil: None, multisample: Default::default(),
        fragment: Some(FragmentState { module: &module, entry_point: Some("main"), compilation_options: Default::default(),
            targets: &[Some(ColorTargetState { format: TextureFormat::Rgba8Unorm, blend: None, write_mask: ColorWrites::ALL })] }),
        multiview_mask: None, cache: None });
    let output = g.device.create_texture(&TextureDescriptor { label: None,
        size: Extent3d { width: 1, height: 1, depth_or_array_layers: 14 }, mip_level_count: 1, sample_count: 1,
        dimension: TextureDimension::D2, format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC, view_formats: &[] });
    let draw = |g: &mut Gpu, layer: u32, tail: bool| {
        let (_, bytes) = color(layer);
        guest_write(g, &bytes);
        g.upload_buffer(1, 0, 16, 1024, false);
        guest_write(g, &[0xcc; 16]); // caller can reuse payload immediately after return
        if tail { g.ubo_off = RING_CHUNK - UBO_WINDOW; }
        let binding = g.stage_bind_group(1, &program, 0, &bgl).unwrap();
        let view = output.create_view(&TextureViewDescriptor { dimension: Some(TextureViewDimension::D2),
            base_array_layer: layer, array_layer_count: Some(1), ..Default::default() });
        let mut pass = g.encoder().begin_render_pass(&RenderPassDescriptor { label: None,
            color_attachments: &[Some(RenderPassColorAttachment { view: &view, depth_slice: None, resolve_target: None,
                ops: Operations { load: LoadOp::Clear(Color::BLACK), store: StoreOp::Store } })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &binding.bg, &[binding.dyn_off.unwrap()]);
        pass.draw(0..3, 0..1);
    };
    // Two chunks, three GPU regions each, two rounds of real PRESENT submission
    // and production completion fences. One staging mirror is shared by all slots.
    for frame in 0..6 {
        let waits = g.slot_waits;
        draw(&mut g, frame * 2, false);
        draw(&mut g, frame * 2 + 1, true);
        assert_eq!(g.slot_waits, waits, "CPU preparation must not wait on the old slot");
        g.present(0);
        assert_eq!(g.slot_waits, waits + u64::from(frame >= 3), "first submission must fence reused slots");
        if frame >= 2 { assert!(g.frame_slot_done[g.ubo_slot].is_some(), "next slot fence remains pending until next submission"); }
    }
    assert_eq!(g.ubo_shared.len(), 2);
    assert_eq!(g.ubo_staging.len(), 2);
    assert!(g.ubo_slots.iter().all(|s| s.len() == 2));
    assert_eq!(g.ubo_staging.iter().map(|s| s.retained_bytes()).sum::<usize>(), 2 * 262144);
    let old_id = g.ubo_shared[1].1;
    assert!(g.bind_groups.values().any(|b| b.uniform == Some(old_id)));
    g.reclaim_cold_uniform_chunk();
    assert_eq!(g.ubo_shared.len(), 2, "recent uniform storage must stay cached");
    g.presents += crate::gpu_memory::COLD_UNIFORM_FRAMES;
    g.reclaim_cold_uniform_chunk();
    assert_eq!(g.ubo_shared.len(), 1);
    assert!(g.ubo_slots.iter().all(|s| s.len() == 1));
    assert!(!g.bind_groups.values().any(|b| b.uniform == Some(old_id)));
    assert_eq!(g.uniform_reclaimed_bytes, 24 * 1024 * 1024 + 262144);
    draw(&mut g, 12, false);
    draw(&mut g, 13, true);
    assert_ne!(g.ubo_shared[1].1, old_id, "retired storage must get a new cache identity");
    g.present(0);
    let pixels = read_layers(&mut g, &output, 14);
    for (i, actual) in pixels.iter().enumerate() {
        for (got, want) in actual.iter().zip(color(i as u32).0) { assert!((*got as i16 - want as i16).abs() <= 1); }
    }

    // Native texture upload borrows the command's bytes only until wgpu copies
    // them. Rewriting the guest region before submission must not change pixels.
    g.create_texture(2, 2, 1, 1, 1, 1, 1, 28, BIND_SRV, 1);
    guest_write(&g, &[13, 41, 79, 255]);
    g.upload_texture(2, 0, 0, 0, 0, 0, 1, 1, 1, 4, 1, 4, 1024);
    guest_write(&g, &[0; 4]);
    g.flush();
    let texture = g.tex(2).unwrap().tex.clone();
    assert_eq!(read_layers(&mut g, &texture, 1), vec![[13, 41, 79, 255]]);
    assert_eq!(crate::gpu_memory::texture_bytes(&texture), 4);

    // Production full-buffer UAV clears. Copies before, between and after the
    // clears prove ordering; nonzero readback offsets preserve guard bytes.
    // Four/five-word sizes cover non-16-byte tails without changing view semantics.
    for size in [4u32, 20, 4096] {
        g.create_buffer(3, size, BIND_UAV, 0, 0);
        g.set_obj(4, Obj::Uav(ViewObj { res: 3, is_buf: true, fmt: 0, vdim: 1,
            a: 0, b: size / 4, c: 0, d: 0, views: HashMap::new() }));
        let source = g.buf(3).unwrap().gpu.as_ref().unwrap().clone();
        let initial = vec![0x5a; size as usize];
        g.queue.write_buffer(&source, 0, &initial);
        let readback = g.device.create_buffer(&BufferDescriptor { label: None, size: size as u64 * 4 + 16,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ, mapped_at_creation: false });
        g.queue.write_buffer(&readback, 0, &vec![0xa5; size as usize * 4 + 16]);
        g.encoder().copy_buffer_to_buffer(&source, 0, &readback, 8, size as u64);
        let first = [0x01020304, 0x89abcdef, 0x3f800000, 0x80000000];
        g.clear_uav(4, first);
        g.encoder().copy_buffer_to_buffer(&source, 0, &readback, 8 + size as u64, size as u64);
        g.clear_uav(4, [0; 4]);
        g.encoder().copy_buffer_to_buffer(&source, 0, &readback, 8 + 2 * size as u64, size as u64);
        let last = [17, 31, 47, 73];
        g.clear_uav(4, last);
        g.encoder().copy_buffer_to_buffer(&source, 0, &readback, 8 + 3 * size as u64, size as u64);
        g.flush();
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(MapMode::Read, move |result| tx.send(result).unwrap());
        g.device.poll(PollType::Wait { submission_index: None, timeout: Some(Duration::from_secs(30)) }).unwrap();
        rx.recv().unwrap().unwrap();
        let bytes = readback.slice(..).get_mapped_range().unwrap();
        assert_eq!(&bytes[..8], &[0xa5; 8]);
        assert_eq!(&bytes[bytes.len() - 8..], &[0xa5; 8]);
        assert_eq!(&bytes[8..8 + size as usize], initial.as_slice());
        for (section, pattern) in [(1, first), (2, [0; 4]), (3, last)] {
            let start = 8 + section * size as usize;
            for (i, word) in bytes[start..start + size as usize].chunks_exact(4).enumerate() {
                assert_eq!(word, pattern[i & 3].to_le_bytes());
            }
        }
        drop(bytes);
        readback.unmap();
        g.destroy(4);
        g.destroy(3);
    }
    assert_eq!(g.uav_staged_clears, 6);
    assert_eq!(g.uav_snapshot_bytes_avoided, 2 * (4 + 20 + 4096));
    assert_eq!(g.uav_snapshot_peak_avoided, 4096);
    assert_eq!(g.errors.load(Ordering::Relaxed), 0);
    eprintln!("Metal production memory paths: 14 correct draw pixels across 3 slots, cold-tail retirement/reallocation with new cache identity, exact borrowed upload pixels; UAV clears 4/20/4096 bytes with two nonzero patterns, zero, ordering and guards; zero validation errors");
}
