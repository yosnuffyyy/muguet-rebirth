//! CPU upload ownership. GPU frame-slot storage and completion fences are separate.
use std::ops::Range;

pub const COLD_UNIFORM_FRAMES: u64 = 120;

#[derive(Default)]
pub struct UniformStage {
    bytes: Vec<u8>,
    dirty: Option<Range<usize>>,
    last_frame: u64,
}

impl UniformStage {
    pub fn prepare(&mut self, offset: usize, size: usize, limit: usize, frame: u64) -> &mut [u8] {
        let end = offset.checked_add(size).filter(|end| *end <= limit)
            .expect("uniform staging range exceeds its GPU chunk");
        if self.bytes.len() < end {
            self.bytes.resize(end.max(262144).next_power_of_two().min(limit), 0);
        }
        self.dirty = Some(match self.dirty.take() {
            Some(r) => r.start.min(offset)..r.end.max(end),
            None => offset..end,
        });
        self.last_frame = frame;
        self.bytes[offset..end].fill(0);
        &mut self.bytes[offset..end]
    }

    /// The caller must synchronously copy this slice before reusing the stage.
    /// wgpu Queue::write_buffer does so, independently of GPU completion.
    pub fn take_dirty(&mut self) -> Option<(u64, &[u8])> {
        let r = self.dirty.take()?;
        let end = r.end.checked_add(3).expect("uniform staging overflow") & !3;
        assert!(end <= self.bytes.len());
        Some((r.start as u64, &self.bytes[r.start..end]))
    }

    pub fn retained_bytes(&self) -> usize { self.bytes.capacity() }

    pub fn cold(&self, frame: u64) -> bool {
        self.dirty.is_none() && frame.saturating_sub(self.last_frame) >= COLD_UNIFORM_FRAMES
    }
}

/// Inline ring payload remains producer-owned but immutable until the consumer
/// publishes its advanced tail. Never retain the borrowed bytes past that point.
pub fn upload_range(memory_len: usize, address: u64, size: u64) -> Option<Range<usize>> {
    let start = usize::try_from(address).ok()?;
    let end = start.checked_add(usize::try_from(size).ok()?)?;
    (end <= memory_len).then_some(start..end)
}

/// Exact existing buffer-UAV clear pattern, assembled in write-only staging.
/// No allocation and no reads from the potentially write-combined destination.
pub fn fill_uav_words(destination: wgpu::WriteOnly<'_, [u8]>, values: [u32; 4]) {
    assert_eq!(destination.len() % 4, 0, "UAV buffer clear must contain complete words");
    let mut pattern = [0u8; 16];
    for (word, value) in pattern.chunks_exact_mut(4).zip(values) {
        word.copy_from_slice(&value.to_le_bytes());
    }
    let (mut chunks, mut tail) = destination.into_chunks::<16>();
    chunks.fill(pattern);
    tail.copy_from_slice(&pattern[..tail.len()]);
}

/// Logical texel payload only, not Metal heap/residency or process footprint.
/// Depth24 allocation layout is implementation-defined: count four bytes/texel.
pub fn texture_bytes(t: &wgpu::Texture) -> u64 {
    let f = t.format();
    let (bw, bh) = f.block_dimensions();
    let bytes = f.block_copy_size(None).unwrap_or(match f {
        wgpu::TextureFormat::Depth32FloatStencil8 => 8,
        _ => 4,
    }) as u64;
    (0..t.mip_level_count()).map(|mip| {
        let size = t.size().mip_level_size(mip, t.dimension());
        size.width.div_ceil(bw) as u64 * size.height.div_ceil(bh) as u64
            * size.depth_or_array_layers as u64 * bytes * t.sample_count() as u64
    }).sum()
}

#[cfg(test)]
#[path = "gpu_memory_tests.rs"]
mod tests;
