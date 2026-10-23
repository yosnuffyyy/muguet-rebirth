//! Growth policy preserves Emscripten's normal over-allocation but never asks
//! Wasmtime for pages outside the imported memory's declared maximum.
const PAGE: u64 = 65_536;
const POLICY_MAX: u64 = 17_179_869_184;

pub fn targets(old: u64, requested: u64, declared_limit: u64) -> Option<[u64; 4]> {
    let limit = declared_limit.min(POLICY_MAX) / PAGE * PAGE;
    let exact = requested.checked_add(PAGE - 1)? / PAGE * PAGE;
    if requested <= old || exact > limit { return None; }
    let mut result = [exact; 4];
    for (i, cut) in [1, 2, 4].into_iter().enumerate() {
        let over = (old as f64 * (1.0 + 0.2 / cut as f64)) as u64;
        let candidate = requested.max(over.min(requested.saturating_add(100_663_296)));
        result[i] = candidate.div_ceil(PAGE).saturating_mul(PAGE).min(limit);
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_policy_and_exact_fallback() {
        let old = 512 << 20;
        let requested = old + 1;
        let plan = targets(old, requested, 4 << 30).unwrap();
        assert_eq!(plan[0], (requested + 100_663_296).div_ceil(PAGE) * PAGE);
        assert!(plan.windows(2).all(|pair| pair[0] >= pair[1]));
        assert_eq!(plan[3], old + PAGE);
        // Model transient speculative-growth failure followed by exact success.
        let mut attempts = 0;
        let granted = plan.into_iter().find(|&size| { attempts += 1; size == old + PAGE });
        assert_eq!(granted, Some(old + PAGE));
        assert_eq!(attempts, 4);
    }
    #[test]
    fn original_four_gib_limit_and_overflow() {
        let limit = 4 << 30;
        assert_eq!(targets(limit - PAGE, limit - PAGE + 1, limit), Some([limit; 4]));
        assert!(targets(limit - PAGE, limit + 1, limit).is_none());
        assert!(targets(limit, limit, limit).is_none());
        assert!(targets(PAGE, u64::MAX, u64::MAX).is_none());
        assert!(targets(0, PAGE + 1, PAGE + 1).is_none());
    }
}
