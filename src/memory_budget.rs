//! Advisory memory pressure policy. Never changes guest limits or live resources.
//! The renderer consumes a coalesced request at its own safe boundary.
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

pub static OS_PRESSURE: AtomicU32 = AtomicU32::new(0);
pub static PRESSURE_EVENTS: AtomicU64 = AtomicU64::new(0);
static RECLAIM: AtomicBool = AtomicBool::new(false);
pub static RECLAIM_REQUESTS: AtomicU64 = AtomicU64::new(0);
pub static GPU_STAGING_BYTES: AtomicU64 = AtomicU64::new(u64::MAX);
pub static GPU_UNIFORM_BYTES: AtomicU64 = AtomicU64::new(u64::MAX);
pub static GPU_BUFFER_BYTES: AtomicU64 = AtomicU64::new(u64::MAX);
pub static GPU_TEXTURE_ESTIMATE_BYTES: AtomicU64 = AtomicU64::new(u64::MAX);
pub static GPU_OBJECTS: AtomicU64 = AtomicU64::new(u64::MAX);
pub static GPU_CENSUS_PRESENT: AtomicU64 = AtomicU64::new(u64::MAX);

/// Called by a utility-queue Dispatch memory-pressure handler. No allocation,
/// lock, GPU operation or logging on the notification callback.
#[no_mangle]
pub extern "C" fn rebirth_memory_pressure(level: u32) {
    OS_PRESSURE.store(level.min(2), Ordering::Relaxed);
    PRESSURE_EVENTS.fetch_add(1, Ordering::Relaxed);
}
pub fn take_reclaim_request() -> bool { RECLAIM.swap(false, Ordering::AcqRel) }
pub fn request_reclaim() {
    if !RECLAIM.swap(true, Ordering::AcqRel) {
        RECLAIM_REQUESTS.fetch_add(1, Ordering::Relaxed);
    }
}

const MIB: u64 = 1 << 20;
#[derive(Default)]
pub struct Policy {
    pub peak_footprint: Option<u64>,
    low_samples: u8,
    last_request: Option<u64>,
    last_severity: u32,
    growth_anchor: Option<(u64, u64)>,
}
impl Policy {
    /// Monotonic seconds, current advisory headroom, and independent footprint.
    /// MAX denotes unknown. Thresholds only request reclamation of *unused*
    /// caches, never reject allocations or assume an entitlement's RAM grant.
    pub fn sample(&mut self, now: u64, available: u64, footprint: u64, os: u32) -> Option<&'static str> {
        if footprint != u64::MAX { self.peak_footprint = Some(self.peak_footprint.unwrap_or(0).max(footprint)); }
        self.low_samples = if available < 256 * MIB { self.low_samples.saturating_add(1) } else { 0 };
        let mut growth = false;
        if footprint != u64::MAX {
            let (at, bytes) = *self.growth_anchor.get_or_insert((now, footprint));
            if now.saturating_sub(at) >= 30 {
                growth = footprint.saturating_sub(bytes) >= 128 * MIB && available < 512 * MIB;
                self.growth_anchor = Some((now, footprint));
            }
        }
        let (severity, reason) = if os >= 2 || available < 128 * MIB {
            (2, "critical pressure/headroom")
        } else if os == 1 {
            (1, "OS memory warning")
        } else if self.low_samples >= 2 {
            (1, "sustained low headroom")
        } else if growth {
            (1, "sustained footprint growth with low headroom")
        } else { (0, "normal") };
        if severity == 0 { return None; }
        // Escalation gets one immediate request; recurring pressure is debounced.
        if self.last_request.is_none_or(|last| now.saturating_sub(last) >= 60)
            || severity > self.last_severity {
            self.last_request = Some(now);
            self.last_severity = severity;
            Some(reason)
        } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_and_unknown_samples_do_not_purge() {
        let mut p = Policy::default();
        for t in 0..100 { assert!(p.sample(t, 1024*MIB, 2000*MIB, 0).is_none()); }
        assert!(p.sample(100, u64::MAX, u64::MAX, 0).is_none());
        assert_eq!(p.peak_footprint, Some(2000*MIB));
        let mut unknown = Policy::default();
        assert!(unknown.sample(0, u64::MAX, u64::MAX, 0).is_none());
        assert_eq!(unknown.peak_footprint, None);
    }
    #[test]
    fn transient_low_headroom_is_ignored_and_sustained_pressure_is_bounded() {
        let mut p = Policy::default();
        assert!(p.sample(0, 200*MIB, 1000*MIB, 0).is_none());
        assert!(p.sample(5, 900*MIB, 1000*MIB, 0).is_none());
        assert!(p.sample(10, 200*MIB, 1000*MIB, 0).is_none());
        assert!(p.sample(15, 200*MIB, 1000*MIB, 0).is_some());
        for t in 16..75 { assert!(p.sample(t, 200*MIB, 1000*MIB, 0).is_none()); }
        assert!(p.sample(75, 200*MIB, 1000*MIB, 0).is_some());
    }
    #[test]
    fn warning_and_critical_escalation_do_not_wait_for_low_headroom() {
        let mut p = Policy::default();
        assert!(p.sample(0, u64::MAX, u64::MAX, 1).is_some());
        assert!(p.sample(1, u64::MAX, u64::MAX, 2).is_some());
        assert!(p.sample(2, u64::MAX, u64::MAX, 2).is_none());
        assert!(p.sample(3, 1024*MIB, 500*MIB, 0).is_none());
        assert!(p.sample(4, 1024*MIB, 500*MIB, 1).is_none());
    }
    #[test]
    fn growth_needs_time_and_memory_risk() {
        let mut p = Policy::default();
        assert!(p.sample(0, 400*MIB, 1000*MIB, 0).is_none());
        assert!(p.sample(29, 400*MIB, 1200*MIB, 0).is_none());
        assert!(p.sample(30, 400*MIB, 1200*MIB, 0).is_some());
        assert!(p.sample(90, 1024*MIB, 1600*MIB, 0).is_none());
    }
    #[test]
    fn zero_headroom_requests_reclaim_without_fake_budget() {
        let mut p = Policy::default();
        assert!(p.sample(0, 0, 2000*MIB, 0).is_some());
        assert!(p.sample(1, 0, 2001*MIB, 0).is_none());
    }
}
