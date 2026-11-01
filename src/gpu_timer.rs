//! GPU execution time per PRESENT from render-pass timestamp queries.
//!
//! Each render pass writes a begin/end timestamp into the query set of the
//! current frame slot. At PRESENT the used range is resolved and copied into a
//! per-slot readback buffer; a slot is only reused after its readback mapped,
//! which also proves the earlier resolve finished. If a slot is still busy the
//! frame is simply not timed (counted as skipped) instead of waiting.
//!
//! Pass durations overlap on Apple GPUs (vertex work of one pass can run while
//! the previous pass shades fragments), so the pass sum is not GPU busy time.
//! The frame span (first pass begin to last pass end) includes GPU idle gaps
//! between submits. Neither number is displayed FPS.
use std::fmt::Write;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use wgpu::*;

pub const PASS_CAPACITY: u32 = 256;
const QUERIES: u32 = PASS_CAPACITY * 2;
const BYTES: u64 = QUERIES as u64 * 8;
const MAP_PENDING: u8 = 0;
const MAP_OK: u8 = 1;
const MAP_FAILED: u8 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameSample {
    pub passes: u32,
    pub invalid: u32,
    pub span_ns: u64,
    pub pass_sum_ns: u64,
    pub max_pass_ns: u64,
    pub max_pass_index: u32,
}

/// `ticks` holds [begin, end] pairs in pass recording order. Zero, all-ones
/// (Metal's counter error value) and reversed pairs are counted as invalid.
pub fn summarize(ticks: &[u64], period_ns: f32) -> FrameSample {
    let mut s = FrameSample::default();
    let (mut first, mut last) = (u64::MAX, 0u64);
    let ns = |t: u64| (t as f64 * period_ns as f64) as u64;
    for (i, pair) in ticks.chunks_exact(2).enumerate() {
        let (b, e) = (pair[0], pair[1]);
        if b == 0 || b == u64::MAX || e == u64::MAX || e < b { s.invalid += 1; continue; }
        s.passes += 1;
        first = first.min(b);
        last = last.max(e);
        let d = ns(e - b);
        s.pass_sum_ns = s.pass_sum_ns.saturating_add(d);
        if d > s.max_pass_ns { s.max_pass_ns = d; s.max_pass_index = i as u32; }
    }
    if s.passes > 0 { s.span_ns = ns(last - first); }
    s
}

#[derive(Default)]
struct Window {
    frames: u64,
    span: Vec<u64>,
    pass_sum_total: u128,
    passes: u64,
    invalid: u64,
    max_pass_ns: u64,
    max_pass_index: u32,
    skipped_busy: u64,
    untimed_passes: u64,
    map_failures: u64,
}

struct Slot {
    set: QuerySet,
    resolve: Buffer,
    read: Buffer,
    pending: Option<(Arc<AtomicU8>, u32)>,
}

pub struct GpuTimer {
    slots: Vec<Slot>,
    period_ns: f32,
    cur: usize,
    used: u32,
    active: bool,
    staged: bool,
    window: Window,
}

impl GpuTimer {
    pub fn new(device: &Device, queue: &Queue, slots: usize) -> Option<Self> {
        if !device.features().contains(Features::TIMESTAMP_QUERY) { return None; }
        let period_ns = queue.get_timestamp_period();
        if !(period_ns > 0.0) { return None; }
        let slots = (0..slots).map(|_| Slot {
            set: device.create_query_set(&QuerySetDescriptor { label: Some("gpu timer"), ty: QueryType::Timestamp, count: QUERIES }),
            resolve: device.create_buffer(&BufferDescriptor { label: Some("gpu timer resolve"), size: BYTES, usage: BufferUsages::QUERY_RESOLVE | BufferUsages::COPY_SRC, mapped_at_creation: false }),
            read: device.create_buffer(&BufferDescriptor { label: Some("gpu timer read"), size: BYTES, usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ, mapped_at_creation: false }),
            pending: None,
        }).collect();
        Some(Self { slots, period_ns, cur: 0, used: 0, active: true, staged: false, window: Window::default() })
    }

    /// Query set and begin index for the next render pass, or None when this
    /// frame is not timed or the per-frame capacity is exhausted.
    pub fn next_pass(&mut self) -> Option<(QuerySet, u32)> {
        if !self.active { return None; }
        if self.used + 2 > QUERIES { self.window.untimed_passes += 1; return None; }
        let i = self.used;
        self.used += 2;
        Some((self.slots[self.cur].set.clone(), i))
    }

    pub fn has_samples(&self) -> bool { self.active && self.used > 0 }

    /// Record the resolve and readback copy. The caller must have ended any
    /// open render pass on `enc` and must submit it before `after_submit`.
    pub fn end_frame(&mut self, enc: &mut CommandEncoder) {
        self.staged = false;
        if !self.active || self.used == 0 { return; }
        let s = &self.slots[self.cur];
        enc.resolve_query_set(&s.set, 0..self.used, &s.resolve, 0);
        enc.copy_buffer_to_buffer(&s.resolve, 0, &s.read, 0, self.used as u64 * 8);
        self.staged = true;
    }

    pub fn after_submit(&mut self) {
        if !std::mem::take(&mut self.staged) { return; }
        let state = Arc::new(AtomicU8::new(MAP_PENDING));
        let flag = state.clone();
        let s = &mut self.slots[self.cur];
        s.read.slice(..self.used as u64 * 8).map_async(MapMode::Read, move |r| {
            flag.store(if r.is_ok() { MAP_OK } else { MAP_FAILED }, Ordering::SeqCst);
        });
        s.pending = Some((state, self.used));
    }

    /// Collect finished readbacks, then start timing slot `next` if it is free.
    pub fn begin_frame(&mut self, next: usize) {
        for s in &mut self.slots {
            let Some((state, used)) = &s.pending else { continue };
            let used = *used;
            match state.load(Ordering::SeqCst) {
                MAP_PENDING => continue,
                MAP_OK => {
                    let sample = {
                        let slice = s.read.slice(..used as u64 * 8);
                        match slice.get_mapped_range() {
                            Ok(range) => {
                                let ticks: Vec<u64> = range.chunks_exact(8).map(|b| u64::from_le_bytes(b.try_into().unwrap())).collect();
                                Some(summarize(&ticks, self.period_ns))
                            }
                            Err(_) => None,
                        }
                    };
                    s.read.unmap();
                    match sample { Some(f) => self.window.add(f), None => self.window.map_failures += 1 }
                }
                _ => self.window.map_failures += 1,
            }
            s.pending = None;
        }
        self.cur = next % self.slots.len();
        self.used = 0;
        self.active = self.slots[self.cur].pending.is_none();
        if !self.active { self.window.skipped_busy += 1; }
    }

    /// Ten-second heartbeat line; resets the window.
    pub fn report(&mut self) -> String {
        let w = std::mem::take(&mut self.window);
        let mut out = String::with_capacity(512);
        let _ = write!(out, "GPU pass timestamps: {} frames timed", w.frames);
        if w.frames > 0 {
            let mut span = w.span;
            span.sort_unstable();
            let pct = |p: usize| span[(p * span.len()).div_ceil(100) - 1] as f64 / 1e6;
            let mean = span.iter().map(|&v| v as u128).sum::<u128>() as f64 / span.len() as f64 / 1e6;
            let _ = write!(out, "; frame GPU span mean/p50/p95/max {:.2}/{:.2}/{:.2}/{:.2} ms; pass-duration sum mean {:.2} ms (passes overlap, not additive); {:.1} timed passes/frame; slowest pass {:.2} ms (pass #{} of its frame)",
                mean, pct(50), pct(95), pct(100), w.pass_sum_total as f64 / w.frames as f64 / 1e6,
                w.passes as f64 / w.frames as f64, w.max_pass_ns as f64 / 1e6, w.max_pass_index);
        }
        let _ = write!(out, "; {} frames skipped (readback busy), {} passes untimed (over {} per frame), {} invalid pairs, {} readback failures. Span includes GPU idle gaps between submits; not displayed FPS.",
            w.skipped_busy, w.untimed_passes, PASS_CAPACITY, w.invalid, w.map_failures);
        out
    }
}

impl Window {
    fn add(&mut self, f: FrameSample) {
        self.invalid += f.invalid as u64;
        if f.passes == 0 { return; }
        self.frames += 1;
        if self.span.len() < 4096 { self.span.push(f.span_ns); }
        self.pass_sum_total += f.pass_sum_ns as u128;
        self.passes += f.passes as u64;
        if f.max_pass_ns > self.max_pass_ns { self.max_pass_ns = f.max_pass_ns; self.max_pass_index = f.max_pass_index; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn span_and_overlapping_pass_sum() {
        let s = summarize(&[100, 200, 150, 400, 400, 450], 1.0);
        assert_eq!((s.passes, s.invalid), (3, 0));
        assert_eq!(s.span_ns, 350);
        assert_eq!(s.pass_sum_ns, 100 + 250 + 50);
        assert_eq!((s.max_pass_ns, s.max_pass_index), (250, 1));
    }
    #[test]
    fn invalid_pairs_are_excluded() {
        let s = summarize(&[0, 10, 50, 40, 5, u64::MAX, u64::MAX, 9, 20, 30], 2.0);
        assert_eq!((s.passes, s.invalid), (1, 4));
        assert_eq!((s.span_ns, s.pass_sum_ns), (20, 20));
    }
    #[test]
    fn empty_and_odd_input() {
        assert_eq!(summarize(&[], 1.0), FrameSample::default());
        assert_eq!(summarize(&[7], 1.0), FrameSample::default());
    }
    #[test]
    fn window_ignores_frames_without_valid_passes() {
        let mut w = Window::default();
        w.add(summarize(&[0, 0], 1.0));
        w.add(summarize(&[10, 30], 1.0));
        assert_eq!((w.frames, w.invalid, w.passes), (1, 1, 1));
        assert_eq!(w.span, vec![20]);
    }
}
