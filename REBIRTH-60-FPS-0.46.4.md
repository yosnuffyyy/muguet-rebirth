# Rebirth 0.46.4 — selectable 60 FPS cap

Executable revision: `35c28af3f9ca736e241cd97dbcc596360a60e079`; version 0.46.4, build 50.

Graphics → Frame cap offers 15, 20, 30 and 60 FPS. Default remains 30. The choice persists, validates in Swift and Rust, and freezes before launch. 60 maps to the existing engine argument `-frameLimit=1` (60 Hz divisor one); the existing iOS refresh request is 60 Hz. Engine timing, physics, renderer and audio logic are unchanged. Change the cap before a new app session.

Both Story and Sandbox use this configuration without changing their other arguments or save namespaces. Legacy migration accepts 60. Unsupported caps remain rejected. Selecting 60 does not guarantee sustained 60 FPS.

## Verification

- Windows/Node pinned original-engine argument fixture: 352 checks across eight adapted launch configurations (two modes, four divisors). Original SetFrameLock stores divisor 1 for 60. This isolated fixture is not full game startup.
- Original timer fixture with its normal CApp startup minimum of 1/3000 second: 10 elapsed seconds at 60 updates/sec yield 9.9999994 seconds summed engine delta and 10,000 engine milliseconds. Static pre-startup defaults differ; no production timer overrides were added. Forced-timing and real gameplay still require device checks.
- Rust regression checks persisted 60 FPS, both modes, exactly one frame cap argument, unchanged other arguments, default 30 and invalid values.
- Swift regression checks repeated persisted cap changes in both modes and legacy migration.
- CI: https://github.com/yosnuffyyy/muguet-rebirth/actions/runs/39700098729 — pending at report preparation.

## Next iPhone test

Install build 50 over the existing app. Select Graphics → Frame cap → 60 FPS, reopen to check persistence, and test the same scene used at 30 FPS. Export the log and confirm frozen `fps:60` and `-frameLimit=1`. Compare actual displayed intervals, controls, audio and animation timing. 16.67 ms is the target interval, not a promised result. Return to 30 if sustained heat or pacing worsens.

IPA: `GTA-Rebirth-0.46.4-35c28af3f9ca-unsigned.ipa`, 63,158,220 bytes, available in `outputs/GTA-Native-Prototype`. SHA256: `323ce4a3b731712892ded3b02cd405036c10bf43ae63a8b408bac8d7b12811c9`. ARM64 UUID: `3F8E6596-1B5E-3540-9359-6D6307B340A9`. Ad-hoc package requires AltStore signing. Both requested memory entitlements were checked in the built executable by CI; actual OS grants are not assumed. GitHub contains the source and encrypted IPA artifact; no web/game assets were uploaded. Local ZIP CRC, architecture, identity and hash checks passed. Prior IPAs and selected game assets remain untouched. No physical-device 60 FPS result is claimed.
