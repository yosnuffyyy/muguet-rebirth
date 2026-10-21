//! Real Metal execution of the production transfer and final-blit code.
use super::*;
fn texture(d:&Device,w:u32,h:u32,mips:u32,layers:u32,samples:u32,f:TextureFormat)->Texture {
    d.create_texture(&TextureDescriptor{label:Some("resolve regression"),size:Extent3d{width:w,height:h,depth_or_array_layers:layers},mip_level_count:mips,sample_count:samples,dimension:TextureDimension::D2,format:f,
        usage:TextureUsages::RENDER_ATTACHMENT|TextureUsages::TEXTURE_BINDING|if samples==1 {TextureUsages::COPY_SRC|TextureUsages::COPY_DST}else{TextureUsages::empty()},view_formats:&[]})
}
fn view(t:&Texture,m:u32,l:u32)->TextureView {t.create_view(&TextureViewDescriptor{dimension:Some(TextureViewDimension::D2),base_mip_level:m,mip_level_count:Some(1),base_array_layer:l,array_layer_count:Some(1),..Default::default()})}
fn clear(e:&mut CommandEncoder,t:&Texture,m:u32,l:u32,c:Color) {
    let v=view(t,m,l);
    let _p=e.begin_render_pass(&RenderPassDescriptor{label:None,color_attachments:&[Some(RenderPassColorAttachment{view:&v,depth_slice:None,resolve_target:None,ops:Operations{load:LoadOp::Clear(c),store:StoreOp::Store}})],depth_stencil_attachment:None,timestamp_writes:None,occlusion_query_set:None,multiview_mask:None});
}
fn read(d:&Device,q:&Queue,t:&Texture,m:u32,l:u32)->Vec<u8> {
    let (w,h)=((t.width()>>m).max(1),(t.height()>>m).max(1));let pitch=(w*4).div_ceil(256)*256;
    let b=d.create_buffer(&BufferDescriptor{label:None,size:(pitch*h) as u64,usage:BufferUsages::COPY_DST|BufferUsages::MAP_READ,mapped_at_creation:false});
    let mut e=d.create_command_encoder(&Default::default());
    e.copy_texture_to_buffer(TexelCopyTextureInfo{texture:t,mip_level:m,origin:Origin3d{x:0,y:0,z:l},aspect:TextureAspect::All},TexelCopyBufferInfo{buffer:&b,layout:TexelCopyBufferLayout{offset:0,bytes_per_row:Some(pitch),rows_per_image:Some(h)}},Extent3d{width:w,height:h,depth_or_array_layers:1});
    q.submit([e.finish()]);let(tx,rx)=std::sync::mpsc::channel();b.map_async(MapMode::Read,..,move |r|tx.send(r).unwrap());
    d.poll(PollType::Wait{submission_index:None,timeout:Some(std::time::Duration::from_secs(30))}).unwrap();rx.recv().unwrap().unwrap();
    let data=b.get_mapped_range(..);let mut out=Vec::new();for y in 0..h {out.extend_from_slice(&data[(y*pitch)as usize..(y*pitch+w*4)as usize]);}drop(data);b.unmap();out
}
fn pipeline(d:&Device,code:&str,f:TextureFormat,n:u32)->RenderPipeline {
    let m=d.create_shader_module(ShaderModuleDescriptor{label:None,source:ShaderSource::Wgsl(code.into())});
    d.create_render_pipeline(&RenderPipelineDescriptor{label:None,layout:None,vertex:VertexState{module:&m,entry_point:Some("vs"),compilation_options:Default::default(),buffers:&[]},primitive:Default::default(),depth_stencil:None,multisample:MultisampleState{count:n,..Default::default()},fragment:Some(FragmentState{module:&m,entry_point:Some("fs"),compilation_options:Default::default(),targets:&[Some(ColorTargetState{format:f,blend:None,write_mask:ColorWrites::ALL})]}),multiview_mask:None,cache:None})
}
const VERTEX:&str="@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {var p=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));return vec4f(p[i],0,1);}";

#[test]
fn metal_resolve_copy_mrt_and_final_blit_pixels() {
    let mut desc=InstanceDescriptor::new_without_display_handle();desc.backends=Backends::METAL;
    let instance=Instance::new(desc);
    let a=pollster::block_on(instance.request_adapter(&RequestAdapterOptions{apply_limit_buckets:false,..Default::default()})).expect("real Metal adapter required; this test must not silently skip");
    let(d,q)=pollster::block_on(a.request_device(&DeviceDescriptor{required_features:Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,..Default::default()})).unwrap();
    let errors=std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));let err=errors.clone();d.on_uncaptured_error(std::sync::Arc::new(move |e|err.lock().unwrap().push(e.to_string())));
    let mut cases=0;
    for (w,h) in [(640,360),(1280,720)] {for f in [TextureFormat::Rgba8Unorm,TextureFormat::Bgra8Unorm] {
        let src=texture(&d,w,h,1,1,1,f);let dst=texture(&d,w,h,1,1,1,f);
        // Vary all bytes, including alpha: the transfer/blit must not invent opacity.
        let bytes:Vec<u8>=(0..w*h).flat_map(|i|[(i%251)as u8,((i/7)%253)as u8,((i/19)%255)as u8,(i%256)as u8]).collect();
        q.write_texture(TexelCopyTextureInfo{texture:&src,mip_level:0,origin:Origin3d::ZERO,aspect:TextureAspect::All},&bytes,TexelCopyBufferLayout{offset:0,bytes_per_row:Some(w*4),rows_per_image:Some(h)},Extent3d{width:w,height:h,depth_or_array_layers:1});
        let mut e=d.create_command_encoder(&Default::default());
        assert_eq!(record(&mut e,&src,(0,0),&dst,(0,0),true).unwrap(),Transfer::Copy);
        assert_eq!(record(&mut e,&dst,(0,0),&dst,(0,0),true).unwrap(),Transfer::Identity);
        q.submit([e.finish()]);assert_eq!(read(&d,&q,&dst,0,0),bytes);assert_eq!(read(&d,&q,&src,0,0),bytes);
        let output=texture(&d,w,h,1,1,1,f);let pl=pipeline(&d,crate::gpu_probe::BLIT,f,1);let sv=view(&dst,0,0);let ov=view(&output,0,0);
        let bg=d.create_bind_group(&BindGroupDescriptor{label:None,layout:&pl.get_bind_group_layout(0),entries:&[BindGroupEntry{binding:0,resource:BindingResource::TextureView(&sv)}]});
        let mut e=d.create_command_encoder(&Default::default());{
            let mut p=e.begin_render_pass(&RenderPassDescriptor{label:None,color_attachments:&[Some(RenderPassColorAttachment{view:&ov,depth_slice:None,resolve_target:None,ops:Operations{load:LoadOp::Clear(Color::BLACK),store:StoreOp::Store}})],depth_stencil_attachment:None,timestamp_writes:None,occlusion_query_set:None,multiview_mask:None});p.set_pipeline(&pl);p.set_bind_group(0,&bg,&[]);p.draw(0..3,0..1);
        }q.submit([e.finish()]);assert_eq!(read(&d,&q,&output,0,0),bytes);cases+=1;
        for n in [2,4] {
            let flags=a.get_texture_format_features(f).flags;
            if !flags.sample_count_supported(n) {eprintln!("Metal {f:?} {n}x unavailable; no claim for this mode");continue;}
            assert!(flags.contains(TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE));
            let ms=texture(&d,w,h,1,1,n,f);
            let code=format!("{VERTEX} @fragment fn fs(@builtin(sample_index) i:u32)->@location(0) vec4f {{return vec4f(f32(i)/{}.0,0.0,1.0,1.0);}}",n-1);
            let pl=pipeline(&d,&code,f,n);let v=view(&ms,0,0);let mut e=d.create_command_encoder(&Default::default());{
                let mut p=e.begin_render_pass(&RenderPassDescriptor{label:None,color_attachments:&[Some(RenderPassColorAttachment{view:&v,depth_slice:None,resolve_target:None,ops:Operations{load:LoadOp::Clear(Color::BLACK),store:StoreOp::Store}})],depth_stencil_attachment:None,timestamp_writes:None,occlusion_query_set:None,multiview_mask:None});p.set_pipeline(&pl);p.draw(0..3,0..1);
            }
            assert_eq!(record(&mut e,&ms,(0,0),&dst,(0,0),true).unwrap(),Transfer::Resolve);
            // A second resolve from the same MSAA source proves source storage survives.
            record(&mut e,&ms,(0,0),&output,(0,0),true).unwrap();q.submit([e.finish()]);
            let pixels=read(&d,&q,&dst,0,0);assert_eq!(pixels,read(&d,&q,&output,0,0));
            let red=if f==TextureFormat::Bgra8Unorm {2}else{0};for px in pixels.chunks_exact(4) {assert!((px[red]as i32-128).abs()<=1);assert_eq!(px[1],0);assert_eq!(px[3],255);}cases+=1;
        }
    }}
    let f=TextureFormat::Rgba8Unorm;let t=texture(&d,32,32,3,2,1,f);let u=texture(&d,32,32,3,2,1,f);let mut e=d.create_command_encoder(&Default::default());
    clear(&mut e,&t,2,1,Color::GREEN);clear(&mut e,&u,2,0,Color::RED);clear(&mut e,&u,2,1,Color::BLUE);
    record(&mut e,&t,(2,1),&u,(2,1),true).unwrap();
    assert!(record(&mut e,&t,(3,0),&u,(0,0),true).is_err());assert!(record(&mut e,&t,(0,2),&u,(0,0),true).is_err());assert!(record(&mut e,&t,(1,0),&u,(0,0),true).is_err());
    let wrong=texture(&d,32,32,1,1,1,TextureFormat::Rgba16Float);assert!(record(&mut e,&t,(0,0),&wrong,(0,0),true).is_err());
    let ms=texture(&d,32,32,1,1,4,f);assert!(record(&mut e,&t,(0,0),&ms,(0,0),true).is_err());assert!(record(&mut e,&ms,(0,0),&u,(0,0),false).is_err());
    q.submit([e.finish()]);assert!(read(&d,&q,&u,2,1).chunks_exact(4).all(|p|p==[0,255,0,255]));assert!(read(&d,&q,&u,2,0).chunks_exact(4).all(|p|p==[255,0,0,255]));
    // Extra PS outputs are legal with unbound MRT slots. No dummy targets/rewrites.
    let code=format!("{VERTEX} struct Out {{@location(0) a:vec4f,@location(1) b:vec4f,@location(2) c:vec4f}}; @fragment fn fs()->Out {{return Out(vec4f(0,1,0,1),vec4f(1),vec4f(1));}}");
    let pl=pipeline(&d,&code,f,1);let v=view(&u,0,0);let mut e=d.create_command_encoder(&Default::default());{
        let mut p=e.begin_render_pass(&RenderPassDescriptor{label:None,color_attachments:&[Some(RenderPassColorAttachment{view:&v,depth_slice:None,resolve_target:None,ops:Operations{load:LoadOp::Clear(Color::BLACK),store:StoreOp::Store}})],depth_stencil_attachment:None,timestamp_writes:None,occlusion_query_set:None,multiview_mask:None});p.set_pipeline(&pl);p.draw(0..3,0..1);
    }q.submit([e.finish()]);assert!(read(&d,&q,&u,0,0).chunks_exact(4).all(|p|p==[0,255,0,255]));
    let collected=errors.lock().unwrap().clone();assert!(collected.is_empty(),"{collected:?}");
    eprintln!("Metal {:?}: {cases} resolution/format/sample cases + array/mip preservation, strict rejection, identity, production final blit, unbound MRT: zero validation errors",a.get_info());
}
