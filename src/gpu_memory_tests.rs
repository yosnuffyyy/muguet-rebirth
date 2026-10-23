use super::*;

#[test]
fn uniform_dirty_ranges_zero_padding_and_cold_lifetime() {
    let mut s = UniformStage::default();
    s.prepare(0, 16, 1024, 5).fill(0xaa);
    s.prepare(256, 12, 1024, 5).fill(0xbb);
    assert!(!s.cold(1000), "dirty bytes must never be retired");
    let (off, bytes) = s.take_dirty().unwrap();
    assert_eq!(off, 0);
    assert_eq!(bytes.len(), 268);
    assert!(bytes[..16].iter().all(|b| *b == 0xaa));
    assert!(bytes[16..256].iter().all(|b| *b == 0));
    assert!(bytes[256..].iter().all(|b| *b == 0xbb));
    assert!(s.take_dirty().is_none());
    assert!(!s.cold(124));
    assert!(s.cold(125));
    assert!(!s.cold(0), "clock regression is conservative");
    assert!(s.prepare(0, 16, 1024, 126).iter().all(|b| *b == 0));
}

#[test]
fn upload_payload_ranges_reject_overflow_and_out_of_bounds() {
    assert_eq!(upload_range(1024, 16, 1008), Some(16..1024));
    assert_eq!(upload_range(1024, 1024, 0), Some(1024..1024));
    assert_eq!(upload_range(1024, 1024, 1), None);
    assert_eq!(upload_range(1024, u64::MAX, 1), None);
    assert_eq!(upload_range(1024, 8, u64::MAX), None);
}

#[test]
#[should_panic(expected = "uniform staging range")]
fn staging_range_cannot_overwrite_adjacent_gpu_slot() {
    UniformStage::default().prepare(1008, 17, 1024, 0);
}

#[test]
fn direct_uav_staging_preserves_pattern_and_write_boundaries() {
    for words in [0, 1, 2, 3, 4, 5, 7, 16, 255, 256, 257] {
        let values = [0x01020304, 0xffffffff, 0x3f800000, 0x80000000];
        let expected: Vec<u8> = (0..words).flat_map(|i| values[i & 3].to_le_bytes()).collect();
        let mut destination = vec![0xaa; words * 4 + 16];
        fill_uav_words(&mut destination[8..8 + words * 4], values);
        assert_eq!(&destination[..8], &[0xaa; 8]);
        assert_eq!(&destination[8 + words * 4..], &[0xaa; 8]);
        assert_eq!(&destination[8..8 + words * 4], expected.as_slice());
    }
}

#[test]
fn synthetic_uav_clear_allocation_and_copy_comparison() {
    const SIZE: usize = 64 << 20;
    let values: [u32; 4] = [0x01234567, 0x89abcdef, 0x3f800000, 0];
    // Stand in for wgpu's required staging allocation in both paths. Keep the
    // old snapshot in a nested scope so before/after do not overlap in the test.
    let mut driver_staging = vec![0u8; SIZE];
    let old_start = std::time::Instant::now();
    let old_host_bytes;
    {
        let snapshot: Vec<u8> = (0..SIZE / 4).flat_map(|i| values[i & 3].to_le_bytes()).collect();
        old_host_bytes = snapshot.capacity();
        driver_staging.copy_from_slice(&snapshot);
    }
    let old_time = old_start.elapsed();
    let checksum = driver_staging.iter().fold(0u64, |sum, b| sum.wrapping_add(*b as u64));
    driver_staging.fill(0);
    let new_start = std::time::Instant::now();
    fill_uav_words(&mut driver_staging, values);
    let new_time = new_start.elapsed();
    assert_eq!(checksum, driver_staging.iter().fold(0u64, |sum, b| sum.wrapping_add(*b as u64)));
    for (i, word) in driver_staging.chunks_exact(4).enumerate() { assert_eq!(word, values[i & 3].to_le_bytes()); }
    assert!(old_host_bytes >= SIZE);
    eprintln!("HOST synthetic UAV clear {SIZE} bytes: old extra snapshot capacity {old_host_bytes}, direct extra snapshot 0, required driver staging {SIZE} in both; old fill/copy {old_time:?}, direct fill {new_time:?}; exact words; not iPhone footprint/FPS");
}

#[test]
fn synthetic_uniform_staging_allocation_and_cpu_comparison() {
    // Identical writes and queue-copy stand-in. This is a host fixture, not FPS
    // or device footprint: it isolates the former three CPU staging owners.
    const CHUNK: usize = 8 << 20;
    const FRAMES: usize = 90;
    let run = |slots: usize| {
        let mut stages: Vec<UniformStage> = (0..slots).map(|_| UniformStage::default()).collect();
        let mut driver_copy = vec![0u8; CHUNK];
        let start = std::time::Instant::now();
        for frame in 0..FRAMES {
            let s = &mut stages[frame % slots];
            for off in (0..CHUNK).step_by(256) {
                s.prepare(off, 256, CHUNK, frame as u64).fill(frame as u8);
            }
            let (offset, data) = s.take_dirty().unwrap();
            driver_copy[offset as usize..offset as usize + data.len()].copy_from_slice(data);
            std::hint::black_box(&driver_copy);
        }
        (start.elapsed(), stages.iter().map(UniformStage::retained_bytes).sum::<usize>(), driver_copy)
    };
    let (old_time, old_bytes, old_output) = run(3);
    let (new_time, new_bytes, new_output) = run(1);
    assert_eq!(old_output, new_output);
    assert_eq!((old_bytes, new_bytes), (24 << 20, 8 << 20));
    eprintln!("HOST synthetic uniform staging {FRAMES} frames/8MiB: old {old_bytes} retained bytes {old_time:?}, shared {new_bytes} retained bytes {new_time:?}; exact output; wall time is not GPU time or iPhone FPS");
}
