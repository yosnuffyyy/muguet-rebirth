//! Opt-in display diagnosis. Eight pairs of five pixels; no full-frame readbacks.
use wgpu::*;
use std::sync::{LazyLock, Mutex};
pub const BLIT: &str = "@group(0) @binding(0) var src: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {var p=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));return vec4f(p[i],0,1);}
@fragment fn fs(@builtin(position) p:vec4f)->@location(0) vec4f {return textureLoad(src,vec2i(p.xy),0);}";
pub static ENABLED: LazyLock<bool> = LazyLock::new(|| std::env::var("MUGUET_DISPLAY_PROBE").is_ok_and(|v| v == "1"));
pub fn selected(n:u64)->bool {[30,60,120,240,480,900,1200,1800].contains(&n)}
pub fn encode(device:&Device, enc:&mut CommandEncoder, tex:&Texture)->Option<Buffer> {
    if tex.sample_count()!=1 || !tex.usage().contains(TextureUsages::COPY_SRC) || !matches!(tex.format(),TextureFormat::Rgba8Unorm|TextureFormat::Bgra8Unorm|TextureFormat::Rgba8UnormSrgb|TextureFormat::Bgra8UnormSrgb) {return None;}
    let buf=device.create_buffer(&BufferDescriptor{label:Some("five diagnostic pixels"),size:1280,usage:BufferUsages::COPY_DST|BufferUsages::MAP_READ,mapped_at_creation:false});
    for i in 0..5 {
        enc.copy_texture_to_buffer(TexelCopyTextureInfo{texture:tex,mip_level:0,origin:Origin3d{x:(tex.width()-1)*i/4,y:(tex.height()-1)*i/4,z:0},aspect:TextureAspect::All},
            TexelCopyBufferInfo{buffer:&buf,layout:TexelCopyBufferLayout{offset:i as u64*256,bytes_per_row:Some(256),rows_per_image:Some(1)}},Extent3d{width:1,height:1,depth_or_array_layers:1});
    }
    Some(buf)
}
static LAST: Mutex<[(u64,u64);2]> = Mutex::new([(0,0);2]);
pub fn finish(buf:Option<Buffer>, stage:usize, frame:u64) {
    let label=if stage==0 {"game-backbuffer"} else {"surface-after-blit"};
    let Some(buf)=buf else {crate::rebirth::logs::checkpoint(&format!("pixel probe {label} frame {frame}: UNKNOWN (format or COPY_SRC unavailable)"),false);return;};
    let owned=buf.clone();
    buf.slice(..).map_async(MapMode::Read,move |result| {
        if let Err(e)=result {crate::rebirth::logs::checkpoint(&format!("pixel probe {label}: map failed {e}"),false);return;}
        let data=match owned.slice(..).get_mapped_range() {Ok(data)=>data,Err(e)=>{crate::rebirth::logs::checkpoint(&format!("pixel probe {label}: mapped range failed {e}"),false);owned.unmap();return;}};
        let pixels:Vec<_>=(0..5).map(|i|<[u8;4]>::try_from(&data[i*256..i*256+4]).unwrap()).collect();
        let nonblack=pixels.iter().filter(|p|p[..3].iter().any(|&b|b!=0)).count();
        let hash=pixels.iter().flatten().fold(14695981039346656037u64,|h,&b|(h^b as u64).wrapping_mul(1099511628211));
        let mut last=LAST.lock().unwrap();
        let changed=(last[stage].0>0 && frame>last[stage].0).then_some(hash!=last[stage].1);
        if frame>last[stage].0 {last[stage]=(frame,hash);}
        drop(last);drop(data);owned.unmap();
        crate::rebirth::logs::checkpoint(&format!("pixel probe {label} frame {frame}: nonblack {nonblack}/5 changed={changed:?} bytes={pixels:?}; sampled pixels only, not display completion"),false);
    });
}
#[cfg(test)] mod tests {
    #[test] fn diagnostic_work_is_bounded() {assert_eq!((0..100_000).filter(|&n|super::selected(n)).count(),8);}
}
