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
        extern "C" {
            fn muguet_memory_monitor_start();
            fn muguet_memory_details(values: *mut u64);
        }
        unsafe { muguet_memory_monitor_start(); }
        let start = std::time::Instant::now();
        let mut policy = crate::memory_budget::Policy::default();
        let mut last_log = None;
        loop {
            use crate::memory_budget as budget;
            let mut detail = [u64::MAX; 6];
            unsafe { muguet_memory_details(detail.as_mut_ptr()); }
            let [footprint, resident, compressed, available, metal, thermal] = detail;
            let elapsed = start.elapsed().as_secs();
            let pressure = budget::OS_PRESSURE.load(Ordering::Relaxed);
            let reason = policy.sample(elapsed, available, footprint, pressure);
            if let Some(reason) = reason {
                budget::request_reclaim();
                crate::rebirth::logs::checkpoint(&format!("memory reclaim requested: {reason}; available_bytes={available} footprint_bytes={footprint}; idle caches only, no guest limit/quality change"), true);
            }
            let period = if elapsed < 120 { 5 } else { 30 };
            if reason.is_some() || last_log.is_none_or(|last| elapsed.saturating_sub(last) >= period) {
                last_log = Some(elapsed);
                let guest = crate::RT.get().map(|rt| rt.mem.data_size());
                crate::rebirth::logs::checkpoint(&format!(
                    "heartbeat elapsed={elapsed}s available_bytes={available} footprint_bytes={footprint} resident_bytes={resident} compressed_bytes={compressed} metal_allocated_bytes={metal} peak_sampled_footprint={} thermal={thermal} os_pressure={pressure} pressure_events={} reclaim_requests={} (MAX=unknown; counters overlap, do not sum) guest_accessible_bytes={guest:?} workers_started/ready={}/{} commands={} present_requests={} surface_present_calls={} script_ticks={} suspended={} audio_callbacks/nonzero_output_samples={}/{}; guest resident/GPU residency/display completion/player spawn UNKNOWN",
                    policy.peak_footprint, budget::PRESSURE_EVENTS.load(Ordering::Relaxed), budget::RECLAIM_REQUESTS.load(Ordering::Relaxed),
                    WORKERS_STARTED.load(Ordering::Relaxed), WORKERS_READY.load(Ordering::Relaxed),
                    GPU_COMMANDS.load(Ordering::Relaxed), PRESENT_REQUESTS.load(Ordering::Relaxed), SURFACE_PRESENTS.load(Ordering::Relaxed), SCRIPT_TICKS.load(Ordering::Relaxed),
                    crate::gpu::SUSPENDED.load(Ordering::Relaxed), crate::audio::CALLBACKS.load(Ordering::Relaxed), crate::audio::NONZERO_SAMPLES.load(Ordering::Relaxed)), true);
                crate::rebirth::logs::checkpoint(&format!(
                    "latest renderer ownership counters (independently sampled; PRESENT marker {}): live_objects={} engine_buffer_capacity_bytes={} texture_payload_estimate_bytes={} uniform_gpu_capacity_bytes={} uniform_cpu_stage_capacity_bytes={}; MAX=not sampled; estimates exclude driver padding/metadata and are not additive with footprint/Metal totals",
                    budget::GPU_CENSUS_PRESENT.load(Ordering::Relaxed), budget::GPU_OBJECTS.load(Ordering::Relaxed), budget::GPU_BUFFER_BYTES.load(Ordering::Relaxed),
                    budget::GPU_TEXTURE_ESTIMATE_BYTES.load(Ordering::Relaxed), budget::GPU_UNIFORM_BYTES.load(Ordering::Relaxed), budget::GPU_STAGING_BYTES.load(Ordering::Relaxed)), false);
            }
            // Advisory background sampling only. Never waits for GPU completion,
            // walks all guest pages or assumes an entitlement's memory grant.
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    }) { crate::rebirth::logs::checkpoint(&format!("heartbeat unavailable: {e}"), true); }
}
