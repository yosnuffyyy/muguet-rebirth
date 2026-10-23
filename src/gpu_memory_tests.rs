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
