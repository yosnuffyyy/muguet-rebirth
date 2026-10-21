//! Tagged input import in the pinned 8b0b5899ed engine (i64 parameter).
//! Negative values are optional browser-controller update hooks, not pointers
//! to the shared keyboard/mouse publication block. Muguet owns controller input
//! through its native keyboard/mouse adapter; there is no browser pad snapshot.
use anyhow::{bail, ensure, Result};
pub const INPUT_BYTES: u64 = 444;
pub const PAD: u64 = 12_840_656;
pub const CONTROLS: [u64; 3] = [24_488_864, 24_660_320, 24_831_776];

#[derive(Debug, PartialEq)]
pub enum InputCall { Publish(u64), BrowserPadHook(u64) }

pub fn decode(raw: i64, bytes: u64) -> Result<InputCall> {
    if raw < 0 {
        let pointer = (!raw) as u64; // -1 - raw, including i64::MIN without overflow
        let span = if pointer == PAD { 56 } else if CONTROLS.contains(&pointer) { 138276 }
                   else { bail!("unknown tagged input callback {raw}") };
        ensure!(pointer.checked_add(span).is_some_and(|end| end <= bytes), "tagged input outside guest memory");
        return Ok(InputCall::BrowserPadHook(pointer));
    }
    let pointer = raw as u64;
    ensure!(pointer != 0 && pointer % 4 == 0, "unaligned/null input publication");
    ensure!(pointer.checked_add(INPUT_BYTES).is_some_and(|end| end <= bytes), "input publication outside guest memory");
    Ok(InputCall::Publish(pointer))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tagged_callbacks_are_not_publications() {
        assert_eq!(decode(0x64748c, 715 << 20).unwrap(), InputCall::Publish(0x64748c));
        for pointer in [PAD, CONTROLS[0], CONTROLS[1], CONTROLS[2]] {
            assert_eq!(decode(-1 - pointer as i64, 715 << 20).unwrap(), InputCall::BrowserPadHook(pointer));
        }
    }
    #[test]
    fn rejects_bad_ranges_and_tags() {
        for raw in [0, 3, -1, i64::MIN, i64::MAX, 0x64748d] { assert!(decode(raw, 715 << 20).is_err()); }
        assert!(decode(4, 447).is_err());
        assert_eq!(decode(4, 448).unwrap(), InputCall::Publish(4));
        assert!(decode(-1 - PAD as i64, PAD + 55).is_err());
    }
}
