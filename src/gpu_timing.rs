//! Bounded wall-time accounting at completed PRESENT boundaries.
//!
//! Categories are nested/overlapping, not additive. In particular producer idle
//! and completion waits are elapsed wall time, never GPU execution or CPU usage.
//! Recording uses fixed storage; sorting/formatting happens only on request.
use std::{fmt::Write, time::{Duration, Instant}};

pub const HISTORY_SAMPLES: usize = 256;
const METRICS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Timing {
    FrameInterval,
    CommandWall,
    SlotWait,
    Submit,
    Acquire,
    Present,
    Poll,
    Readback,
    PipelineWait,
    ProducerIdle,
}
impl Timing {
    const ALL: [Self; METRICS] = [
        Self::FrameInterval, Self::CommandWall, Self::SlotWait, Self::Submit,
        Self::Acquire, Self::Present, Self::Poll, Self::Readback,
        Self::PipelineWait, Self::ProducerIdle,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::FrameInterval => "PRESENT interval",
            Self::CommandWall => "command handler wall",
            Self::SlotWait => "frame-slot completion wait",
            Self::Submit => "queue submit API wall",
            Self::Acquire => "surface acquire wall",
            Self::Present => "surface present API wall",
            Self::Poll => "device poll wall",
            Self::Readback => "readback wall",
            Self::PipelineWait => "pipeline wait wall",
            Self::ProducerIdle => "producer idle wall",
        }
    }
}

#[derive(Clone, Copy)]
struct History {
    samples: [u64; HISTORY_SAMPLES],
    next: usize,
    len: usize,
    lifetime_samples: u64,
    lifetime_events: u64,
    lifetime_total_ns: u128,
    lifetime_max_ns: u64,
}
impl History {
    const fn new() -> Self {
        Self { samples: [0; HISTORY_SAMPLES], next: 0, len: 0,
            lifetime_samples: 0, lifetime_events: 0,
            lifetime_total_ns: 0, lifetime_max_ns: 0 }
    }
    fn push(&mut self, nanos: u64, events: u64) {
        self.samples[self.next] = nanos;
        self.next = (self.next + 1) % HISTORY_SAMPLES;
        self.len = (self.len + 1).min(HISTORY_SAMPLES);
        self.lifetime_samples = self.lifetime_samples.saturating_add(1);
        self.lifetime_events = self.lifetime_events.saturating_add(events);
        self.lifetime_total_ns = self.lifetime_total_ns.saturating_add(nanos as u128);
        self.lifetime_max_ns = self.lifetime_max_ns.max(nanos);
    }
    fn snapshot(&self) -> TimingSnapshot {
        let mut result = TimingSnapshot {
            recent_samples: self.len,
            lifetime_samples: self.lifetime_samples,
            lifetime_events: self.lifetime_events,
            lifetime_total_ns: self.lifetime_total_ns,
            lifetime_max_ns: self.lifetime_max_ns,
            ..TimingSnapshot::default()
        };
        if self.len == 0 { return result; }
        let mut sorted = self.samples;
        let values = &mut sorted[..self.len];
        values.sort_unstable();
        let total: u128 = values.iter().map(|&v| v as u128).sum();
        // Nearest-rank percentiles, including one-sample and zero-valued frames.
        let percentile = |p: usize| values[(p * values.len()).div_ceil(100) - 1];
        result.recent_mean_ns = total as f64 / self.len as f64;
        result.recent_p50_ns = percentile(50);
        result.recent_p95_ns = percentile(95);
        result.recent_p99_ns = percentile(99);
        result.recent_max_ns = *values.last().unwrap();
        result
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TimingSnapshot {
    pub recent_samples: usize,
    pub lifetime_samples: u64,
    pub lifetime_events: u64,
    pub recent_mean_ns: f64,
    pub recent_p50_ns: u64,
    pub recent_p95_ns: u64,
    pub recent_p99_ns: u64,
    pub recent_max_ns: u64,
    pub lifetime_total_ns: u128,
    pub lifetime_max_ns: u64,
}

pub struct FrameTelemetry {
    history: [History; METRICS],
    pending_ns: [u64; METRICS],
    pending_events: [u64; METRICS],
    last_frame: Option<Instant>,
    completed_frames: u64,
}
impl Default for FrameTelemetry {
    fn default() -> Self { Self::new() }
}
impl FrameTelemetry {
    pub const fn new() -> Self {
        Self { history: [History::new(); METRICS], pending_ns: [0; METRICS],
            pending_events: [0; METRICS], last_frame: None, completed_frames: 0 }
    }

    /// Aggregate each event into the current PRESENT frame. A direct
    /// FrameInterval record is an explicit interval sample; normally only
    /// finish_frame generates these. Do not mix both interval sources.
    pub fn record(&mut self, timing: Timing, duration: Duration) {
        let index = timing as usize;
        let nanos = duration.as_nanos().min(u64::MAX as u128) as u64;
        if timing == Timing::FrameInterval {
            self.history[index].push(nanos, 1);
        } else {
            self.pending_ns[index] = self.pending_ns[index].saturating_add(nanos);
            self.pending_events[index] = self.pending_events[index].saturating_add(1);
        }
    }

    /// Caller supplies its monotonic completed-PRESENT boundary. The first
    /// boundary commits category sums but has no preceding interval. Missing
    /// category events contribute zero to that frame, not a missing sample.
    /// A reversed timestamp omits its interval and preserves the last valid
    /// anchor; it never manufactures a zero-time frame or negative duration.
    pub fn finish_frame(&mut self, now: Instant) {
        if let Some(previous) = self.last_frame {
            if let Some(interval) = now.checked_duration_since(previous) {
                self.record(Timing::FrameInterval, interval);
                self.last_frame = Some(now);
            }
        } else { self.last_frame = Some(now); }
        for index in 1..METRICS {
            self.history[index].push(self.pending_ns[index], self.pending_events[index]);
            self.pending_ns[index] = 0;
            self.pending_events[index] = 0;
        }
        self.completed_frames = self.completed_frames.saturating_add(1);
    }

    /// Uses a fixed stack copy for sorting; no allocations or mutation.
    pub fn snapshot(&self, timing: Timing) -> TimingSnapshot {
        self.history[timing as usize].snapshot()
    }

    /// Intended for the existing ten-second diagnostics boundary only.
    pub fn report(&self) -> String {
        let mut output = String::with_capacity(3072);
        let _ = writeln!(output, "Renderer wall telemetry: {} completed PRESENTs; latest <=256 samples/category. Categories overlap and are not additive; event counts are timed calls/segments, not frames or protocol-command counts. Command wall excludes between-batch service work and diagnostic formatting. PRESENT intervals are not displayed FPS; waits/API wall are not GPU execution or active CPU time.", self.completed_frames);
        for timing in Timing::ALL {
            let sample = self.snapshot(timing);
            if sample.recent_samples == 0 {
                let _ = write!(output, "{}: no completed samples", timing.label());
            } else {
                let _ = write!(output, "{}: latest {} mean/p50/p95/p99/max {:.3}/{:.3}/{:.3}/{:.3}/{:.3} ms; lifetime {} samples, {} events, {:.3} ms total, {:.3} ms max",
                    timing.label(), sample.recent_samples, sample.recent_mean_ns / 1e6,
                    sample.recent_p50_ns as f64 / 1e6, sample.recent_p95_ns as f64 / 1e6,
                    sample.recent_p99_ns as f64 / 1e6, sample.recent_max_ns as f64 / 1e6,
                    sample.lifetime_samples, sample.lifetime_events,
                    sample.lifetime_total_ns as f64 / 1e6, sample.lifetime_max_ns as f64 / 1e6);
            }
            let index = timing as usize;
            if self.pending_events[index] != 0 {
                let _ = write!(output, "; unfinished frame {} events, {:.3} ms", self.pending_events[index], self.pending_ns[index] as f64 / 1e6);
            }
            output.push('\n');
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_and_unfinished_frames_are_not_fake_samples() {
        let mut telemetry = FrameTelemetry::new();
        let empty = telemetry.snapshot(Timing::CommandWall);
        assert_eq!(empty.recent_samples, 0);
        assert!(empty.recent_mean_ns.is_finite());
        telemetry.record(Timing::CommandWall, Duration::from_millis(7));
        assert_eq!(telemetry.snapshot(Timing::CommandWall).recent_samples, 0);
        assert!(telemetry.report().contains("unfinished frame 1 events, 7.000 ms"));
    }
    #[test]
    fn single_frame_and_zero_activity_frames_keep_event_counts_separate() {
        let mut telemetry = FrameTelemetry::new();
        let start = Instant::now();
        telemetry.record(Timing::CommandWall, Duration::from_millis(3));
        telemetry.record(Timing::CommandWall, Duration::from_millis(5));
        telemetry.finish_frame(start);
        let first = telemetry.snapshot(Timing::CommandWall);
        assert_eq!(first.recent_samples, 1);
        assert_eq!(first.lifetime_events, 2);
        assert_eq!((first.recent_p50_ns, first.recent_p99_ns), (8_000_000, 8_000_000));
        assert_eq!(telemetry.snapshot(Timing::FrameInterval).recent_samples, 0);
        telemetry.finish_frame(start + Duration::from_millis(30));
        let next = telemetry.snapshot(Timing::CommandWall);
        assert_eq!(next.recent_samples, 2);
        assert_eq!(next.lifetime_events, 2);
        assert_eq!(next.recent_mean_ns, 4_000_000.0);
        assert_eq!(next.recent_p50_ns, 0);
        assert_eq!(next.lifetime_total_ns, 8_000_000);
        assert_eq!(telemetry.snapshot(Timing::FrameInterval).recent_p99_ns, 30_000_000);
    }
    #[test]
    fn ring_wrap_preserves_lifetime_totals_and_latest_quantiles() {
        let mut telemetry = FrameTelemetry::new();
        for nanos in 1..=300 { telemetry.record(Timing::FrameInterval, Duration::from_nanos(nanos)); }
        let recent = telemetry.snapshot(Timing::FrameInterval);
        assert_eq!(recent.recent_samples, HISTORY_SAMPLES);
        assert_eq!(recent.lifetime_samples, 300);
        assert_eq!(recent.lifetime_events, 300);
        assert_eq!(recent.lifetime_total_ns, 45_150);
        assert_eq!(recent.recent_mean_ns, 172.5);
        assert_eq!((recent.recent_p50_ns, recent.recent_p95_ns, recent.recent_p99_ns, recent.recent_max_ns), (172, 288, 298, 300));
    }
    #[test]
    fn nested_categories_remain_separate_and_timestamps_are_monotonic() {
        let mut telemetry = FrameTelemetry::new();
        let start = Instant::now();
        telemetry.finish_frame(start);
        telemetry.record(Timing::CommandWall, Duration::from_millis(10));
        telemetry.record(Timing::Poll, Duration::from_millis(4));
        telemetry.record(Timing::Readback, Duration::from_millis(6));
        telemetry.record(Timing::ProducerIdle, Duration::from_millis(2));
        telemetry.finish_frame(start + Duration::from_millis(16));
        assert_eq!(telemetry.snapshot(Timing::FrameInterval).recent_max_ns, 16_000_000);
        assert_eq!(telemetry.snapshot(Timing::CommandWall).lifetime_total_ns, 10_000_000);
        assert_eq!(telemetry.snapshot(Timing::Poll).lifetime_total_ns, 4_000_000);
        assert_eq!(telemetry.snapshot(Timing::Readback).lifetime_total_ns, 6_000_000);
        assert!(telemetry.report().contains("overlap and are not additive"));
        telemetry.finish_frame(start + Duration::from_millis(8));
        assert_eq!(telemetry.snapshot(Timing::FrameInterval).lifetime_samples, 1);
        telemetry.finish_frame(start + Duration::from_millis(32));
        assert_eq!(telemetry.snapshot(Timing::FrameInterval).lifetime_total_ns, 32_000_000);
    }
    #[test]
    fn extreme_durations_and_totals_remain_finite() {
        let mut telemetry = FrameTelemetry::new();
        for _ in 0..4 { telemetry.record(Timing::CommandWall, Duration::MAX); }
        telemetry.finish_frame(Instant::now());
        let snapshot = telemetry.snapshot(Timing::CommandWall);
        assert_eq!(snapshot.recent_max_ns, u64::MAX);
        assert!(snapshot.recent_mean_ns.is_finite());
        let report = telemetry.report();
        assert!(!report.contains("NaN") && !report.contains(" inf"));
    }
    #[test]
    fn synthetic_million_record_overhead() {
        let mut telemetry = FrameTelemetry::new();
        let start = Instant::now();
        for _ in 0..1_000_000 {
            std::hint::black_box(&mut telemetry).record(Timing::CommandWall, Duration::from_nanos(1));
        }
        let elapsed = start.elapsed();
        telemetry.finish_frame(Instant::now());
        let sample = telemetry.snapshot(Timing::CommandWall);
        assert_eq!(sample.lifetime_events, 1_000_000);
        assert_eq!(sample.lifetime_total_ns, 1_000_000);
        println!("[timing-fixture] 1000000 synthetic record calls: {:?}, {:.3} ns/call; fixed telemetry {} bytes, excludes clock reads/report sorting/formatting; host microbenchmark, not iPhone FPS",
            elapsed, elapsed.as_nanos() as f64 / 1_000_000.0, std::mem::size_of::<FrameTelemetry>());
    }
}
