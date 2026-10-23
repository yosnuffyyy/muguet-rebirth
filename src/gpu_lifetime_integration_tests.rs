//! Real Metal execution through production view/bind-group/destruction paths.
use super::*;

fn fixture() -> Gpu {
    let mut config = wasmtime::Config::new();
    config.wasm_threads(true).shared_memory(true);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let ty = wasmtime::MemoryTypeBuilder::new().min(4).max(Some(4)).shared(true).build().unwrap();
    let mem = SharedMemory::new(&engine, ty).unwrap();
    Gpu::new(mem, 0, 65536, 0, None).expect("real Metal adapter required, no silent skip")
}

fn program(g: &mut Gpu, source: &str) -> Program {
    let module = g.device.create_shader_module(ShaderModuleDescriptor { label: None, source: ShaderSource::Wgsl(source.into()) });
    let info = parse_wgsl(source);
    let mut groups = BTreeMap::new();
    groups.insert(0, info.bindings.clone());
    Program { module, info, consts: None, pack: None, text: source.into(), flat: vec![],
        group_bindings: groups, variants: RefCell::new(HashMap::new()), label: "resource lifetime regression".into(),
        uid: g.uid(), stable: 1, hash: String::new(), vid: false, iid: false, out_locs: vec![0], in_locs: vec![], clip: false }
}

fn view(g: &mut Gpu, id: u32, resource: u32, buffer: bool, uav: bool) {
    let v = ViewObj { res: resource, is_buf: buffer, fmt: 0, vdim: if buffer { 1 } else { 4 },
        a: 0, b: if buffer { 4 } else { 1 }, c: 0, d: 0, views: HashMap::new() };
    g.set_obj(id, if uav { Obj::Uav(v) } else { Obj::Srv(v) });
}

fn upload(g: &mut Gpu, id: u32, color: [u8; 4]) {
    unsafe { std::ptr::copy_nonoverlapping(color.as_ptr(), g.base.add(1024), 4); }
    g.upload_texture(id, 0, 0, 0, 0, 0, 1, 1, 1, 4, 1, 4, 1024);
}

fn render_pipeline(g: &Gpu, p: &Program, bgl: &BindGroupLayout) -> RenderPipeline {
    let layout = g.device.create_pipeline_layout(&PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(bgl)], immediate_size: 0 });
    g.device.create_render_pipeline(&RenderPipelineDescriptor { label: None, layout: Some(&layout),
        vertex: VertexState { module: &p.module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
        primitive: Default::default(), depth_stencil: None, multisample: Default::default(),
        fragment: Some(FragmentState { module: &p.module, entry_point: Some("main"), compilation_options: Default::default(),
            targets: &[Some(ColorTargetState { format: TextureFormat::Rgba8Unorm, blend: None, write_mask: ColorWrites::ALL })] }),
        multiview_mask: None, cache: None })
}

fn output(g: &Gpu, layers: u32) -> Texture {
    g.device.create_texture(&TextureDescriptor { label: None,
        size: Extent3d { width: 1, height: 1, depth_or_array_layers: layers }, mip_level_count: 1, sample_count: 1,
        dimension: TextureDimension::D2, format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC, view_formats: &[] })
}

fn draw(g: &mut Gpu, p: &Program, bgl: &(BindGroupLayout, u64), pipeline: &RenderPipeline, output: &Texture, layer: u32) -> u64 {
    let binding = g.stage_bind_group(1, p, 0, bgl).unwrap();
    let target = output.create_view(&TextureViewDescriptor { dimension: Some(TextureViewDimension::D2),
        base_array_layer: layer, array_layer_count: Some(1), ..Default::default() });
    let mut pass = g.encoder().begin_render_pass(&RenderPassDescriptor { label: None,
        color_attachments: &[Some(RenderPassColorAttachment { view: &target, depth_slice: None, resolve_target: None,
            ops: Operations { load: LoadOp::Clear(Color::BLACK), store: StoreOp::Store } })],
        depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &binding.bg, &[]);
    pass.draw(0..3, 0..1);
    binding.uid
}

fn mapped(g: &mut Gpu, buffer: &Buffer) -> Vec<u8> {
    g.flush();
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(MapMode::Read, move |result| tx.send(result).unwrap());
    g.device.poll(PollType::Wait { submission_index: None, timeout: Some(Duration::from_secs(30)) }).unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    bytes
}

fn pixels(g: &mut Gpu, texture: &Texture, layers: u32) -> Vec<[u8; 4]> {
    let buffer = g.device.create_buffer(&BufferDescriptor { label: None, size: 256 * layers as u64,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ, mapped_at_creation: false });
    g.encoder().copy_texture_to_buffer(TexelCopyTextureInfo { texture, mip_level: 0, origin: Origin3d::ZERO, aspect: TextureAspect::All },
        TexelCopyBufferInfo { buffer: &buffer, layout: TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(1) } },
        Extent3d { width: 1, height: 1, depth_or_array_layers: layers });
    let bytes = mapped(g, &buffer);
    (0..layers).map(|i| bytes[i as usize * 256..i as usize * 256 + 4].try_into().unwrap()).collect()
}

const TEXTURE_SHADER: &str = "@group(0) @binding(32) var image: texture_2d<f32>;
    @vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
        var p = array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3)); return vec4f(p[i],0,1);
    }
    @fragment fn main() -> @location(0) vec4f { return textureLoad(image,vec2i(0),0); }";

#[test]
fn metal_texture_and_view_id_reuse_does_not_reuse_old_pixels() {
    let mut g = fixture();
    let p = program(&mut g, TEXTURE_SHADER);
    let bgl = g.bgl_for(&p, 1, 0);
    let pipeline = render_pipeline(&g, &p, &bgl.0);
    let out = output(&g, 4);
    g.create_texture(10, 2, 1, 1, 1, 1, 1, 28, BIND_SRV, 1);
    view(&mut g, 20, 10, false, false);
    g.s.srv[1][0] = 20;
    upload(&mut g, 10, [255, 0, 0, 255]);
    let old = draw(&mut g, &p, &bgl, &pipeline, &out, 0);
    assert!(!g.view_obj(20).unwrap().views.is_empty());
    // Replacement without a preceding explicit destroy is also a generation
    // boundary. Keep the descriptor to prove its native view is invalidated.
    g.create_texture(10, 2, 1, 1, 1, 1, 1, 28, BIND_SRV, 1);
    assert!(g.view_obj(20).unwrap().views.is_empty());
    assert!(!g.bind_groups.values().any(|entry| entry.uid == old));
    upload(&mut g, 10, [0, 255, 0, 255]);
    let current = draw(&mut g, &p, &bgl, &pipeline, &out, 1);
    assert_ne!(old, current);
    assert_eq!(draw(&mut g, &p, &bgl, &pipeline, &out, 2), current, "live group remains a cache hit");
    g.create_texture(11, 2, 1, 1, 1, 1, 1, 28, BIND_SRV, 1);
    upload(&mut g, 11, [0, 0, 255, 255]);
    view(&mut g, 20, 11, false, false);
    assert!(!g.bind_groups.values().any(|entry| entry.uid == current));
    draw(&mut g, &p, &bgl, &pipeline, &out, 3);
    g.destroy(20); g.destroy(10); g.destroy(11);
    assert_eq!(g.bind_groups.len(), 0);
    assert_eq!(g.view_owners.len(), 0);
    assert_eq!(pixels(&mut g, &out, 4), vec![[255,0,0,255],[0,255,0,255],[0,255,0,255],[0,0,255,255]]);
    assert_eq!(g.errors.load(Ordering::Relaxed), 0);
}

#[test]
fn metal_obsolete_depth_proxy_cache_releases_ownership_without_breaking_inflight_pixels() {
    let mut g = fixture();
    let source = TEXTURE_SHADER.replace("return textureLoad(image,vec2i(0),0);", "let d=textureLoad(image,vec2i(0),0).r; return vec4f(d,d,d,1);");
    let p = program(&mut g, &source);
    let bgl = g.bgl_for(&p, 1, 0);
    let pipeline = render_pipeline(&g, &p, &bgl.0);
    let out = output(&g, 2);
    for (layer, depth) in [(0, 0.25), (1, 0.75)] {
        g.create_texture(10, 2, 1024, 1024, 1, 1, 1, 40, BIND_SRV | BIND_DS, 1);
        view(&mut g, 20, 10, false, false);
        g.s.srv[1][0] = 20;
        g.clear_ds(10, 0, 0, 1, depth, 0);
        draw(&mut g, &p, &bgl, &pipeline, &out, layer);
        assert_eq!(crate::gpu_memory::texture_bytes(&g.tex(10).unwrap().proxy_depth.as_ref().unwrap().tex), 4 << 20);
        assert_eq!(g.bind_groups.len(), 1);
        // No completion wait. Existing destroy flushes the resource's encoder;
        // queued work retains the proxy independently of the cache being removed.
        g.destroy(10);
        assert_eq!(g.bind_groups.len(), 0);
        assert_eq!(g.bind_groups.owner_count(), 0);
        g.destroy(20);
    }
    let result = pixels(&mut g, &out, 2);
    for (actual, expected) in result.iter().zip([64i16, 191]) {
        for channel in &actual[..3] { assert!((*channel as i16 - expected).abs() <= 1); }
        assert_eq!(actual[3], 255);
    }
    assert_eq!(g.retired_derived_payload, 8 << 20);
    assert_eq!(g.retired_binding_groups, 2);
    assert_eq!(g.errors.load(Ordering::Relaxed), 0);
    eprintln!("METAL production lifetime: two obsolete 4MiB depth proxies lose all cache owners at destroy; 8MiB logical payload retired, exact pixels preserved; physical residency not measured");
}

#[test]
fn metal_uav_counter_and_buffer_replacement_preserves_submitted_generations() {
    let mut g = fixture();
    let p = program(&mut g, "@group(0) @binding(160) var<storage,read_write> data: array<u32>;
        @group(0) @binding(161) var<storage,read_write> output: array<u32>;
        @group(0) @binding(176) var<storage,read_write> count: array<atomic<u32>>;
        @compute @workgroup_size(1) fn main() { output[0]=data[0]; output[1]=atomicLoad(&count[0]); }");
    let bgl = g.bgl_for(&p, 2, 0);
    let layout = g.device.create_pipeline_layout(&PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bgl.0)], immediate_size: 0 });
    let pipeline = g.device.create_compute_pipeline(&ComputePipelineDescriptor { label: None, layout: Some(&layout), module: &p.module,
        entry_point: Some("main"), compilation_options: Default::default(), cache: None });
    g.create_buffer(11, 16, BIND_UAV, 0, 4);
    view(&mut g, 21, 11, true, true);
    g.s.uav[2][1] = 21;
    let readback = g.device.create_buffer(&BufferDescriptor { label: None, size: 16,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST, mapped_at_creation: false });
    let mut previous_uid = 0;
    for (generation, value, counter) in [(0, 0x12345678u32, 31u32), (1, 0xabcdef01u32, 73u32)] {
        g.create_buffer(10, 16, BIND_UAV, 0, 4);
        view(&mut g, 20, 10, true, true);
        g.s.uav[2][0] = 20;
        let data = g.buf(10).unwrap().gpu.as_ref().unwrap().clone();
        g.queue.write_buffer(&data, 0, &value.to_le_bytes());
        g.set_uav_counter(20, counter);
        let binding = g.stage_bind_group(2, &p, 0, &bgl).unwrap();
        assert_ne!(binding.uid, previous_uid);
        previous_uid = binding.uid;
        {
            let mut pass = g.encoder().begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&pipeline); pass.set_bind_group(0, &binding.bg, &[]); pass.dispatch_workgroups(1,1,1);
        }
        let result = g.buf(11).unwrap().gpu.as_ref().unwrap().clone();
        g.encoder().copy_buffer_to_buffer(&result, 0, &readback, generation * 8, 8);
        drop(binding);
        g.destroy(20); g.destroy(10);
        assert_eq!(g.bind_groups.len(), 0);
    }
    let bytes = mapped(&mut g, &readback);
    let expected: Vec<u8> = [0x12345678u32,31,0xabcdef01,73].into_iter().flat_map(u32::to_le_bytes).collect();
    assert_eq!(bytes, expected);
    assert_eq!(g.retired_derived_payload, 32);
    assert_eq!(g.errors.load(Ordering::Relaxed), 0);
}

#[test]
fn metal_sampler_id_reuse_rebuilds_derived_nearest_sampler() {
    let mut g = fixture();
    let source = TEXTURE_SHADER
        .replace("@group(0) @binding(32)", "@group(0) @binding(16) var smp: sampler; @group(0) @binding(32)")
        .replace("textureLoad(image,vec2i(0),0)", "textureSampleLevel(image,smp,vec2f(1.25,0.5),0)");
    let p = program(&mut g, &source);
    // Exercise the same derived-nearest sampler cache used when the original
    // shader's filtering sampler is paired with an unfilterable resource.
    g.unfilt.insert(p.uid, (0, 1u128 << 16));
    let bgl = g.bgl_for(&p, 1, 0);
    let pipeline = render_pipeline(&g, &p, &bgl.0);
    let out = output(&g, 2);
    g.create_texture(10, 2, 2, 1, 1, 1, 1, 28, BIND_SRV, 1);
    let colors = [255u8,0,0,255, 0,0,255,255];
    unsafe { std::ptr::copy_nonoverlapping(colors.as_ptr(), g.base.add(1024), colors.len()); }
    g.upload_texture(10, 0, 0, 0, 0, 0, 2, 1, 1, 8, 1, 8, 1024);
    view(&mut g, 20, 10, false, false);
    g.s.srv[1][0] = 20;
    g.s.smp[1][0] = 30;
    let mut previous = 0;
    for (layer, address) in [(0, 1), (1, 3)] {
        let mut words = vec![0; 13];
        words[0] = 0x14; // requested linear, converted to nearest by the real path
        words[1] = address; words[2] = 3; words[3] = 3;
        let sampler = g.make_sampler(&words);
        let stable = stable_hash(&words);
        g.set_obj(30, Obj::State(Rc::new(StateObj { words, sampler: Some(sampler), stable })));
        assert!(!g.nearest.contains_key(&30));
        let uid = draw(&mut g, &p, &bgl, &pipeline, &out, layer);
        assert_ne!(uid, previous);
        previous = uid;
        assert!(g.nearest.contains_key(&30));
    }
    g.destroy(30);
    assert!(!g.nearest.contains_key(&30));
    assert_eq!(g.bind_groups.len(), 0);
    assert_eq!(pixels(&mut g, &out, 2), vec![[255,0,0,255],[0,0,255,255]]);
    assert_eq!(g.errors.load(Ordering::Relaxed), 0);
}
