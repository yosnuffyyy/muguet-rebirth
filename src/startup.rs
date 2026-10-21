//! Startup evidence only: counters never drive engine or rendering behavior.
use std::sync::atomic::{AtomicU64, Ordering};
pub static SCRIPT_TICKS: AtomicU64 = AtomicU64::new(0);
pub static WORKERS_STARTED: AtomicU64 = AtomicU64::new(0);
pub static WORKERS_READY: AtomicU64 = AtomicU64::new(0);
pub static GPU_COMMANDS: AtomicU64 = AtomicU64::new(0);
pub static PRESENT_REQUESTS: AtomicU64 = AtomicU64::new(0);
pub static SURFACE_PRESENTS: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "ios")]
pub fn monitor() {
    use std::sync::atomic::AtomicBool;
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::Relaxed) { return; }
    if let Err(e) = std::thread::Builder::new().name("startup-evidence".into()).spawn(|| {
        extern "C" { fn muguet_memory_snapshot(footprint: *mut u64) -> u64; }
        let start = std::time::Instant::now();
        loop {
            let mut footprint = u64::MAX;
            let available = unsafe { muguet_memory_snapshot(&mut footprint) };
            let guest = crate::RT.get().map(|rt| rt.mem.data_size());
            crate::rebirth::logs::checkpoint(&format!(
                "heartbeat elapsed={}s available_bytes={available} footprint_bytes={footprint} (MAX=unknown) guest_accessible_bytes={guest:?} workers_started/ready={}/{} commands={} present_requests={} surface_present_calls={} script_ticks={} suspended={} audio_callbacks/nonzero_output_samples={}/{}; guest resident/GPU resident/display completion/player spawn UNKNOWN",
                start.elapsed().as_secs(), WORKERS_STARTED.load(Ordering::Relaxed), WORKERS_READY.load(Ordering::Relaxed),
                GPU_COMMANDS.load(Ordering::Relaxed), PRESENT_REQUESTS.load(Ordering::Relaxed), SURFACE_PRESENTS.load(Ordering::Relaxed), SCRIPT_TICKS.load(Ordering::Relaxed),
                crate::gpu::SUSPENDED.load(Ordering::Relaxed), crate::audio::CALLBACKS.load(Ordering::Relaxed), crate::audio::NONZERO_SAMPLES.load(Ordering::Relaxed)), true);
            // Advisory samples only; no artificial timeout and no assumed RAM grant.
            std::thread::sleep(std::time::Duration::from_secs(if start.elapsed().as_secs() < 120 { 5 } else { 30 }));
        }
    }) { crate::rebirth::logs::checkpoint(&format!("heartbeat unavailable: {e}"), true); }
}
