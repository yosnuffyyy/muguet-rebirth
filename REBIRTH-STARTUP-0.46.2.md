# Rebirth 0.46.2 / build 48 — startup input ABI and sideload memory

Baseline: c168abd36e45ece7f50feed986cade84640c05ef (0.46.1). Original engine SHA256 e65bec476e8fd8125515150c24037c9bd0d0c46530e3544f9d389ab108b5592e. Wasmtime 49 / Cranelift and wgpu 30 / Metal retained.

## Confirmed defect and targeted fix

The supplied Story 640x360 and Sandbox 1280x720 snapshots stop near ControlMgr startup. The pinned engine contains two signed-i64 callbacks to wasm_input_publish_js: CControlMgr::Update (function 65444) passes -12840657, and CControl::Update (65359) passes -1 minus its control pointer. These are optional browser-controller hooks. The reference JS decodes them separately and does nothing when no browser controller snapshot is supplied.

The fork cast every callback argument to u64 and published it as the keyboard/mouse memory address. A negative hook therefore replaced the valid 0x64748c pointer with an enormous invalid address before atomic stores. This is a confirmed native memory-safety bug and matches the observed startup stage. No OS crash/Jetsam report was supplied, so the precise cause of the recorded process endings is not independently established.

The host now decodes the signed contract, validates known tagged targets and positive publication alignment/range before touching state, and leaves the native Muguet keyboard/mouse controller adapter in control. Known browser-only hooks preserve all original bytes and the published pointer; unknown tags fail explicitly. This is the reference no-browser-snapshot behavior, not a fabricated controller state. No engine binary patches, renderer rewrite, or game-input mapping changes.

The original-module CI check verifies the two actual instruction sequences and calls the real host dispatcher through a Wasmtime i64 import. It alternates modes and resolutions, checks held/released keys, pointer/mode stability and invalid inputs. This proves the repaired bridge, not player spawn or playable Story/Sandbox. Existing save migration/isolation tests remain enabled.

## Diagnostics

Ordered bounded checkpoints cover main TLS/constructors, worker entry/TLS/mailbox readiness, input publication, command ring, first command, first PRESENT request, first surface present call, and the original GtaThread::Update script hook. A 5-second startup heartbeat (30 seconds after two minutes) records footprint, live OS headroom, guest accessible address length, worker/command/present/script counts, audio callbacks/nonzero output samples and suspension. It does not impose a deadline or change memory budgets.

Checkpoint records bypass the asynchronous stdout pipe; fatal records and periodic snapshots sync to disk. Rust panics are captured best-effort with a nonblocking writer lock; no signal handlers are installed. Main traps propagate as errors instead of falling through to a success-status process exit. Worker startup failures and GPU errors/device loss retain evidence. Disk failure, process kill before a record, native access violations and OS termination remain outside guaranteed capture. Full logs retain the existing 8 x 32 MiB bound.

Empty previous session headers can represent opening the launcher without PLAY. New sessions explicitly mark that stage. A snapshot ending at a thread name is not proof that the named thread caused a failure.

A returned queue.present call is NOT drawable/display completion. Actual display completion, GPU residency and player/camera/streaming correctness remain unmeasured. Script hooks are genuine update entries, not proof of successful mission progression. Audio callback/nonzero PCM counters do not prove audible speaker output.

## Entitlements and actual memory

Both ios/Muguet.entitlements and ios/Muguet-sideload.entitlements request:
- com.apple.developer.kernel.increased-memory-limit = true
- com.apple.developer.kernel.extended-virtual-addressing = true

Xcode project.yml and IPA packaging use the sideload file. Packaging reads the actual codesign signature and fails if either Boolean entitlement is missing. signed-entitlements.json records the verified signature. The ad-hoc IPA still needs AltStore signing; original IPAs and the bundle identity are preserved.

The supplied log's 3376 MB is footprint plus instantaneous headroom, not unused RAM. New logs separate os_proc_available_memory() and task phys_footprint in bytes, with both installed entitlement states (1 yes / 0 no / -1 unknown). Guest accessible bytes are not physical commitment. No fixed entitlement-based RAM allowance, heap increase or forced resource purge is introduced. The same allocation/error paths apply without a grant; extra memory cannot be assumed to fix an unrelated pointer bug.

AltStore Classic 2.2+ release notes advertise increased-memory-limit support. Whether the signer/profile preserves BOTH keys must be checked after installation. Launch the installed app, press PLAY, then export Diagnostics: verify the installed-entitlements checkpoint and live headroom. If either key is 0 or unknown, do not infer a grant. If a re-signed IPA/app is available on macOS, inspect its executable with codesign -d --entitlements :- Payload/GTA-Rebirth.app; inspect embedded.mobileprovision with security cms -D -i and compare its Entitlements. Checking our pre-sign IPA alone cannot prove AltStore preservation. Extended virtual address space is not extra resident RAM.

Sources: https://developer.apple.com/documentation/os/os_proc_available_memory ; https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.kernel.increased-memory-limit ; https://github.com/altstoreio/FAQ/blob/main/release-notes/altstore.md

## Device verification still required

Install over the existing bundle without deleting data. Test clean/repeated launches: Story and Sandbox, each at 640x360 and 1280x720. Preserve both save roots. Verify the negative-hook checkpoint is followed by more engine progress, script ticks and surface presentations; visually verify world, player, camera, controls and audio. Export each session. Confirm live headroom and both signed entitlement states.

If it closes, export the next-launch retained log and the matching Settings > Privacy & Security > Analytics & Improvements > Analytics Data crash/Jetsam report, matched by timestamp/build/UUID. No matching OS termination report is currently available. A still-alive app with increasing heartbeat timestamps but stationary progress counters indicates a stall requiring further investigation; do not label it jetsam.

Validation/build identity will be recorded from CI after completion. No physical iPhone success is claimed here.
