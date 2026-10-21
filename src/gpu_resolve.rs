//! Opcode 36: the engine protocol also uses resolve for a single-sample transfer.
//! Validate before recording anything, so an invalid pass cannot poison the encoder.
use anyhow::{ensure, Result};
use wgpu::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transfer { Identity, Copy, Resolve }

pub fn subresource(mips: u32, layers: u32, dropped: u32, sub: u32) -> Result<(u32, u32)> {
    let original = mips.checked_add(dropped).filter(|&n| n != 0).ok_or_else(|| anyhow::anyhow!("invalid mip count"))?;
    ensure!(sub / original < layers, "subresource {sub} outside {layers} layers / {original} original mips");
    let mip = sub % original;
    ensure!(mip >= dropped, "subresource {sub} selects a dropped mip; transfer cannot be reconstructed");
    Ok((mip - dropped, sub / original))
}

pub fn record(enc: &mut CommandEncoder, src: &Texture, ss: (u32,u32), dst: &Texture, ds: (u32,u32), resolve_supported: bool) -> Result<Transfer> {
    ensure!(src.dimension() == TextureDimension::D2 && dst.dimension() == TextureDimension::D2, "opcode 36 requires 2D subresources");
    for (t, (m,l)) in [(src,ss),(dst,ds)] {
        ensure!(m < t.mip_level_count() && l < t.depth_or_array_layers(), "mip/layer {m}/{l} outside texture");
    }
    let extent = |t: &Texture, m: u32| ((t.width() >> m).max(1), (t.height() >> m).max(1));
    let (w,h) = extent(src, ss.0);
    ensure!((w,h) == extent(dst, ds.0), "source/destination mip extents differ");
    ensure!(dst.sample_count() == 1, "destination must have one sample (got {})", dst.sample_count());
    let sf = src.format(); let df = dst.format();
    ensure!(sf.remove_srgb_suffix() == df.remove_srgb_suffix(), "incompatible transfer formats {sf:?} -> {df:?}");
    if src == dst && ss == ds { return Ok(Transfer::Identity); }
    if src.sample_count() == 1 {
        ensure!(src.usage().contains(TextureUsages::COPY_SRC) && dst.usage().contains(TextureUsages::COPY_DST), "single-sample transfer requires COPY_SRC/COPY_DST");
        let (bw,bh) = sf.block_dimensions();
        enc.copy_texture_to_texture(
            TexelCopyTextureInfo {texture:src,mip_level:ss.0,origin:Origin3d{x:0,y:0,z:ss.1},aspect:TextureAspect::All},
            TexelCopyTextureInfo {texture:dst,mip_level:ds.0,origin:Origin3d{x:0,y:0,z:ds.1},aspect:TextureAspect::All},
            Extent3d{width:w.div_ceil(bw)*bw,height:h.div_ceil(bh)*bh,depth_or_array_layers:1});
        return Ok(Transfer::Copy);
    }
    ensure!(sf == df && resolve_supported, "unsupported MSAA resolve format {sf:?} -> {df:?}");
    ensure!(src.usage().contains(TextureUsages::RENDER_ATTACHMENT) && dst.usage().contains(TextureUsages::RENDER_ATTACHMENT), "MSAA resolve requires render attachments");
    let view = |t: &Texture, (m,l)| t.create_view(&TextureViewDescriptor {dimension:Some(TextureViewDimension::D2),base_mip_level:m,mip_level_count:Some(1),base_array_layer:l,array_layer_count:Some(1),..Default::default()});
    let sv=view(src,ss); let dv=view(dst,ds);
    let _pass = enc.begin_render_pass(&RenderPassDescriptor {
        label:Some("opcode 36 MSAA resolve"),
        color_attachments:&[Some(RenderPassColorAttachment {view:&sv,depth_slice:None,resolve_target:Some(&dv),ops:Operations{load:LoadOp::Load,store:StoreOp::Store}})],
        depth_stencil_attachment:None,timestamp_writes:None,occlusion_query_set:None,multiview_mask:None,
    });
    Ok(Transfer::Resolve)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn original_subresources_are_not_clamped() {
        assert_eq!(subresource(3,6,1,23).unwrap(),(2,5));
        for n in [0,4,24,u32::MAX] {assert!(subresource(3,6,1,n).is_err());}
        assert!(subresource(0,1,0,0).is_err());
        assert!(subresource(u32::MAX,1,1,1).is_err());
    }
}

#[cfg(test)]
#[path = "gpu_resolve_tests.rs"]
mod metal_tests;
