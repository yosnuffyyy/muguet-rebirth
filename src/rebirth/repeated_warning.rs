//! Aggregate one captured repetitive warning, never general errors or assertions.
//! The original first line, periodic totals and final pending count remain visible.
use std::sync::{Mutex, OnceLock};
pub const CLOTH_WARNING: &str = "Warning: <ClothInstanc> [Graphics] grcBuffer_d3d11::Unlock_Internal()...GPU version of buffer not updated!";
const PERIOD_MS: u64 = 5000;
#[derive(Default)]
struct Counts {
    total: u64,
    pending: u64,
    last_ms: u64,
}
impl Counts {
    fn record(&mut self, now_ms: u64) -> (bool, Option<(u64, u64)>) {
        self.total = self.total.saturating_add(1);
        if self.total == 1 {
            self.last_ms = now_ms;
            return (true, None);
        }
        self.pending = self.pending.saturating_add(1);
        if now_ms.saturating_sub(self.last_ms) >= PERIOD_MS {
            self.last_ms = now_ms;
            return (false, self.take());
        }
        (false, None)
    }
    fn take(&mut self) -> Option<(u64, u64)> {
        if self.pending == 0 {
            return None;
        }
        Some((std::mem::take(&mut self.pending), self.total))
    }
}
static COUNTS: Mutex<Counts> = Mutex::new(Counts {
    total: 0,
    pending: 0,
    last_ms: 0,
});
fn eligible(line: &str) -> bool {
    line == CLOTH_WARNING
}
fn summary(value: Option<(u64, u64)>) {
    if let Some((repeats, total)) = value {
        super::logs::checkpoint(&format!("repeated exact ClothInstanc buffer-unlock warning: additional={repeats} lifetime={total}; original warning retained, cause not resolved"), false);
    }
}
pub fn print_engine_line(line: &str) {
    if !eligible(line) {
        eprintln!("[eng] {line}");
        return;
    }
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    let now = START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis()
        .min(u64::MAX as u128) as u64;
    let (original, counts) = COUNTS.lock().unwrap_or_else(|e| e.into_inner()).record(now);
    if original {
        eprintln!("[eng] {line}");
    }
    summary(counts);
}
pub fn flush() {
    let value = COUNTS.lock().unwrap_or_else(|e| e.into_inner()).take();
    summary(value);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_warning_retains_first_and_exact_total() {
        let mut counts = Counts::default();
        assert_eq!(counts.record(0), (true, None));
        for i in 1..711 {
            assert_eq!(counts.record(i), (false, None));
        }
        assert_eq!(counts.take(), Some((710, 711)));
        assert_eq!(counts.take(), None);
        assert_eq!(counts.record(5000), (false, Some((1, 712))));
        assert_eq!(counts.record(4999), (false, None));
        assert_eq!(counts.take(), Some((1, 713)));
    }
    #[test]
    fn only_exact_captured_line_is_eligible() {
        for other in [
            "Fatal Error: GPU version of buffer not updated!",
            "Warning: new error",
            "Error: [Script] startup(37256)",
            "Error: [Script] startup(37296)",
            "[wasm] engine thread 32: ClothInstanceSched:0",
        ] {
            assert!(!eligible(other));
        }
        assert!(!eligible(&format!("{CLOTH_WARNING} new context")));
        assert!(eligible(CLOTH_WARNING));
    }
}
