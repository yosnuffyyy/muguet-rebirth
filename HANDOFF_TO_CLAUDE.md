# Project Rebirth — Claude Code Handoff

**Prepared from the checkout on 2026-10-26.** This document records observed source and repository state; feature claims below are labeled by evidence type. It is intentionally uncommitted. No build or tests were run while preparing it.

## 1. Project identity and repository state

- Active repository: `https://github.com/yosnuffyyy/muguet-rebirth.git`
- Upstream: `https://github.com/c22dev/muguet.git`
- Branch: `native-ios-prototype`; deployed `main` is the upstream baseline and must remain unchanged.
- Checkout: `C:/Users/yosnuffy/Documents/Codex/2026-10-11/https-gta-deloskiyt-pl-https-gta/work/muguet-fork`
- HEAD: `2c239b2b05c88698dfaa1c47c739b3f94786f19b` (`Stabilize graphics settings UI scrolling tests`, 2026-10-23).
- This is the actual Git root. The user's opened workspace one level above it is an archive containing multiple older prototypes and output evidence; do not assume its root is the active repo.
- Last committed source touching the label redesign is `bd33584` (`Restore explicit graphics setting labels and test accessible two-column launcher controls`), on `origin/native-ios-prototype`. The local HEAD adds a test/UI follow-up.

### Initial working-tree state (before this document)

`git status --short --untracked-files=all` showed exactly:

```text
 M ios/RebirthLauncher.swift
 M tests/LauncherGraphicsUITests.swift
?? REBIRTH-0.46.7-GRAPHICS-LABELS.md
```

These are pre-existing user changes and must be preserved. The launcher diff reduces vertical row padding from 6 to 4 points and adds 64 points of scroll tail. The UI test diff changes scrolling/reveal and interaction handling. The untracked results note records previous UI-test limits and IPA provenance. Creating this handoff adds one more untracked file; no other changes were made.

There is no `AGENTS.md` in this Git root. The parent workspace's `AGENTS.md` applies: native iPhone work belongs in `outputs/GTA-Native-Prototype` or independent `work/` directories; preserve IPAs; do not touch `C:\Users\yosnuffy\Desktop\GTA-Browser-Offline-PC` or the separate old desktop package; keep deployed `main` unchanged.

## 2. Current build and version identity

- `ios/Info.plist` declares display name `GTA V (Test Build)`, version **0.46.7**, build **53**. `ios/project.yml` retains bundle ID `io.github.yosnuffyyy.gtanativeprobe`.
- A previously packaged IPA exists outside this Git repository at `../../outputs/GTA-Native-Prototype/GTA-Rebirth-0.46.7-12dedc0084cf-unsigned.ipa` (relative to the repo). SHA-256 verified during this handoff: `5be7e8e43afbfe763c1611c91788590913f5342f189baa16b2b81a8b417b2003`.
- That IPA embeds source revision `12dedc0084cff19e80408c0d00e463e1d3d74949`, **not** current HEAD `2c239b2...` and not the current working-tree edits. Do not describe it as packaging the present checkout.
- The last recorded package-only workflow, run `40224770057`, succeeded at that older source revision. Per user request, it skipped tests. Earlier UI runs are documented as failing scroll/hittability assertions or timing out. Simulator previews showed labels; no physical iPhone UI verification is recorded.
- Current phone-installed version is **unknown**. The 0.46.7 UI displays installed bundle version/build/source using `RebirthBuildIdentity` (`ios/RebirthSettings.swift:5-19`, `ios/RebirthLauncher.swift:97-99,123`).
- Package script writes `build/GTA-Rebirth-...-unsigned.ipa`, but the repository has no current IPA under its `build/` directory. That directory currently contains screenshots/evidence archives. The existing IPA is in the parent workspace output folder.
- IPA is ad-hoc signed by packaging and still needs the user's regular AltStore signing/install flow. An unsigned filename does not mean there is no ad-hoc signature.

## 3. Architecture (source-verified)

This fork is a native iOS host for the original GTA V WebAssembly engine, not the earlier wasm2c/native-C renderer project.

1. Game code: pinned original engine SHA-256 `e65bec476e8fd8125515150c24037c9bd0d0c46530e3544f9d389ab108b5592e`; shader build `8b0b5899ed`. The proprietary game module/assets are user-provided and are not in Git.
2. Runtime: Rust Muguet fork; `Cargo.toml` pins Wasmtime `49` and wgpu `30.0.1`. iOS CI builds with `--no-default-features`, AOT-compiles the game module at build time and links it into ARM64; on-device JIT/Cranelift is not included. The integration workflow explicitly checks that Cranelift, `wgpu-native` and `wasm2c` are absent from the iOS dependency tree (`.github/workflows/rebirth.yml:48-61`).
3. Graphics: Muguet Rust GPU translation through wgpu 30 to Metal (`src/gpu.rs`; resolve handling in `src/gpu_resolve.rs`). The legacy renderer, Renderer V2 and CPU Core V2 are not linked per `REBIRTH.md:7-15,23-29`.
4. Host: winit/UIKit app lifecycle and Rust host ABI (`src/app.rs`, `src/host.rs`, `src/input.rs`, `src/config.rs`). Original engine imports are linked by the host. Engine initialization checks the original shared-memory ABI in `src/main.rs:181-183`.
5. Launcher/config: native SwiftUI (`ios/RebirthLauncher.swift`) plus typed persisted settings (`ios/RebirthSettings.swift`) that are validated/frozen again in Rust (`src/rebirth/settings.rs`, `src/rebirth/mod.rs`). The selected `web` folder is accessed through a security-scoped bookmark; archives and shader index are read in place (`ios/RebirthLauncher.swift:13-37`, `src/rebirth/layout.rs`, `assets.rs`).
6. Audio: guest-published PCM is consumed by a bounded native output path using CPAL/CoreAudio (`src/audio.rs:14-38`). The source reports output startup, but that alone does not prove audible content. `REBIRTH.md` says speaker playback remains device verification work.
7. Input: native touch and GameController/UI input flows into Muguet host/engine input (`src/touch.rs`, `src/input.rs`). Upstream owns much of controller behavior; launcher exposes validated look sensitivity, not every requested control preference.
8. Saves: Story and Sandbox use distinct Rebirth save namespaces. First-use copies from legacy namespaces are bounded and byte-verified; original legacy directories are preserved (`src/rebirth/saves.rs:13-105`, `src/rebirth/assets.rs`). No save payload conversion is claimed.
9. Memory/diagnostics: `src/memory_budget.rs`, `src/gpu_memory.rs`, `src/gpu_lifetime.rs`, `src/rebirth/session.rs`, `logs.rs` track advisory pressure, GPU census, cache ownership and bounded persistent outcome/log data. These do not provide a complete physical GPU residency or per-allocation callstack profiler.

## 4. Launcher and settings state

Graphics controls include presets, base resolution, render scale, 15/20/30/60 FPS choices, texture quality, FXAA, shadow/reflection/water/particle/grass/post-processing/SSAO, anisotropy and frame statistics. The config model validates allowed values and computes scaled dimensions (`ios/RebirthSettings.swift:40-100`; Rust validation and argument mapping in `src/rebirth/settings.rs`). Settings persist in UserDefaults, are saved as one typed model and passed as a frozen JSON launch contract. Engine arguments are logged as requested overrides; logs explicitly say runtime readback/gameplay effectiveness still requires verification (`src/rebirth/mod.rs:32-72`).

The FPS setting maps to Muguet's `-frameLimit` divisor in `src/rebirth/settings.rs`/`src/config.rs`; mapping a launcher selection is source evidence, not proof of stable actual 60 FPS on an iPhone. The launcher labels MSAA, >60/uncapped, native-display resolution and dynamic upscaling unavailable (`ios/RebirthLauncher.swift:148-161`). Do not add placebo options.

### Graphics labels bug: cause and status

The visible symptom was setting values without corresponding names. The earlier SwiftUI composition relied on a `Menu`/`Picker` label within a plain layout; SwiftUI's menu-style control did not reliably render that title as the row label. The current source comment records the issue (`ios/RebirthLauncher.swift:43-45`). The fix is a reusable `LauncherSettingRow` that explicitly renders the left label and right control, plus an explicit current-value view in the picker (`:45-95`); all graphics options use it (`:148-161`). The current row uses `.primary`, wraps long text, and has minimum touch height.

Current local follow-up changes adjust row padding and add scroll room, and modify UI test scrolling/interaction. Those changes are not in the last IPA. Existing graphics UI tests cover row names/values, settings interaction/persistence and accessibility fixtures (`tests/LauncherGraphicsUITests.swift`). **Full UI test pass is not established:** results note reports earlier runs reached scroll/hittability failures and eventually timed out. Do not claim this bug is fully device-validated.

## 5. What works and the evidence boundary

| Capability | Evidence and confidence |
|---|---|
| Customized launcher, settings model, validation, Story/Sandbox choice | Source + Swift/Rust tests in workflow; does not prove actual mission behavior. |
| Original game module imports/TLS/constructors and Muguet host startup | Integration workflow has a bounded original-module initialization step. Constructor/worker startup is explicitly not proof of first gameplay frame. |
| Separate Story/Sandbox save roots and safe legacy-copy design | Source and host tests; actual long-session save behavior needs device validation. |
| wgpu/Metal device and shader index startup | Reported in user-provided Rebirth session text; actual final scene pixels/drawable were not verified in the supplied snapshots. |
| CoreAudio output starts | User-provided log says it starts; stream creation is not proof of nonzero PCM or audible device output. |
| Graphics-setting labels | Current code explicitly draws labels; simulator previews were reported. Full test suite failed/timed out, physical-device check pending. |
| 0.46.7 IPA packaging | Package-only CI success recorded; IPA hash checked. No tests in that package-only run. |
| Sustained gameplay/performance, 60 FPS, crash-free sessions | UNKNOWN for current source/build. No matched current-build iPhone measurement. |

An earlier supplied Muguet log showed both mode 1 and 2 reaching worker/render initialization but stopped at `ControlMgr Update Thread`; it did not prove gameplay. It also contained `Unknown option 'wasmstartchoice=...'`, so do not claim that mode argument itself proved Story/Sandbox state. Later notes describe the same limitation. Device/version association for those snapshots is not reliable.

## 6. Relevant source and build map

- UI/branding: `ios/RebirthLauncher.swift`, `ios/RebirthSettings.swift`, `ios/Info.plist`, `ios/project.yml`, `ios/Muguet*.entitlements`.
- Rust startup and launch contract: `src/main.rs`, `src/app.rs`, `src/config.rs`, `src/rebirth/{mod,settings,layout,assets,saves}.rs`.
- Game ABI, input and threads: `src/host.rs`, `src/input.rs`, `src/touch.rs`, upstream thread/runtime code in `src/`.
- GPU: `src/gpu.rs`, `src/gpu_resolve.rs`, `src/gpu_memory.rs`, `src/gpu_lifetime.rs`, related GPU tests.
- Audio: `src/audio.rs`, `src/opus.rs`.
- Logs/outcome: `src/rebirth/logs.rs`, `src/rebirth/session.rs`, `src/rebirth/repeated_warning.rs`.
- Settings tests/UI fixtures: `tests/RebirthSettingsTests.swift`, `tests/LauncherGraphicsUITests.swift`, `tests/LauncherPreview.swift`; Rust tests live in modules and `tests/`.
- Build/package: `build.sh`, `make-ios.sh`, `tools/original-engine.mjs`, `tools/package-ios.py`, `tools/ios_entitlements.py`, `tools/prepare-icons.mjs`, `.github/workflows/rebirth.yml`.
- Reports: `REBIRTH.md`, `REBIRTH-RESULTS.md`, `REBIRTH-0.46.6-STABILITY-PERFORMANCE.md`, `REBIRTH-0.46.7-GRAPHICS-LABELS.md`, `REBIRTH-MAXIMUM-RAM-OPTIMIZATION.md`, `REBIRTH-BLACK-SCREEN-0.46.3.md`.

Useful current source anchors: `src/main.rs:171-183,271-303,429-475`; `src/host.rs:109-175,330-348`; `src/gpu.rs:1424,3490-3517,3811-3908,3917-4001`; `src/memory_budget.rs:1-64`; `src/rebirth/session.rs:1-85`; `src/rebirth/mod.rs:32-72`.

## 7. Memory/performance and termination evidence

### Latest detailed but historical device evidence

The 0.46.6 report says the complete `Israelrebirth-1792783711571988000-0.txt` was unavailable when investigated. These are user-transcribed device observations for the 0.46.5 / build 51 era, not independently parsed current-build data (`REBIRTH-0.46.6-STABILITY-PERFORMANCE.md:11-29`):

- Process footprint: **3,482,259,960 bytes** (~3,320.9 MiB).
- Available-memory headroom: **57,732,616 bytes** (~55.1 MiB); an earlier session reportedly ~21 MB.
- Metal `currentAllocatedSize`: **1,681,276,928 bytes** (~1,603.4 MiB), not physical GPU residency.
- Estimated texture payload: **1,036,797,532 bytes** (~988.8 MiB); engine buffer capacity: **364,577,512 bytes** (~347.7 MiB); live renderer objects **58,078**.
- Reported cadence fell about 60 to 36.9; source cadence metric counts PRESENT requests and is not equivalent to displayed FPS. Reported stalls **315–634 ms**, not identified as GPU execution time.
- Thermal state was serious. No matched OS Jetsam, `.ips`, EXC_RESOURCE or symbolicated crash report was available. Abrupt termination cause remains **unconfirmed**; memory pressure makes OS termination plausible but does not prove it.

Earlier 0.46.5 report observations include guest accessible memory (~1.80 GB), guest allocator arena (~1.59 GiB) and compressed memory (~776 MiB) sampled at distinct scopes/times. Do not add these to footprint/Metal counters. The full old black-screen log has a known build/settings scope; it is not evidence for the latest renderer. Avoid presenting older measurements as 0.46.7 results.

### Implemented memory/scheduling work (0.46.6 source; host fixtures, not device savings)

`REBIRTH-0.46.6-STABILITY-PERFORMANCE.md:30-110` documents:

- Generation/dependency-aware invalidation for bind groups, views and derived samplers on GPU object destruction/replacement (`src/gpu_lifetime.rs` and `src/gpu.rs`). It retires dependent cache ownership but does not free engine-live assets or promise driver memory release.
- Defers the exact frame-slot completion fence until before GPU uniform writes/submission, allowing CPU command preparation to overlap. Three slots and ordering remain; it does not remove the required wait or guarantee less stutter.
- Removes unused manifest path lookup maps after validation, keeps ordered ID entries.
- Persists bounded process outcome snapshots and reports an unfinished session as cause-unknown; no signal handler/per-frame journal.
- Reuses a single CPU uniform staging area rather than 3 copies (up to 16 MiB host capacity saved in a fully populated fixture; GPU regions unchanged), borrows immutable upload spans until wgpu copies them, uses bounded 64 KiB filesystem scratch, and streams pipeline recipe reads.
- Host fixtures show synthetic cache ownership/time improvements. No post-fix iPhone footprint/FPS/free-headroom numbers exist. Cold-uniform pressure reclaim only releases an eligible trailing chunk; pressure events can reclaim **zero bytes** if no cold chunk qualifies.

## 8. Known issues and remaining uncertainty

1. **Graphics settings UI:** layout rows are implemented but current uncommitted scroll/test changes need review and a complete simulator UI test pass. Test timeouts are real evidence gaps. Confirm all rows on small landscape devices and Dynamic Type.
2. **Current build provenance:** existing 0.46.7 IPA is from `12dedc...`; checkout is `2c239b...` plus local modifications. It must not be installed as a test of current local edits.
3. **Gameplay and mode verification:** mode/config path exists, separate save roots exist, but the log snapshots ending after ControlMgr do not prove Story or Sandbox gameplay. Test each mode with actual game assets, spawn, movement, camera and save separation.
4. **Abrupt exit:** no OS termination report is available. Persistent session journal distinguishes explicit user stop/engine fatal/panic and unfinished run, but unfinished journal cannot identify Jetsam versus user force-close, native crash or watchdog. Obtain device Analytics `.ips`/Jetsam and correlate timestamp/build UUID.
5. **Black display / resolve:** opcode 36 has sample-count-aware identity/copy/resolve handling and strict errors in source (`src/gpu_resolve.rs`, `src/gpu.rs:3490-3517`); prior black-screen log predates that fix. Real game nonblack pixels and visible UIKit presentation remain device-only. The optional magenta display probe is explicitly diagnostic, not gameplay evidence.
6. **Memory:** pressure policy may have no reclaimable objects; live GTA resources cannot be purged safely without engine lifetime signals. Current resident guest pages, actual GPU residency, per-owner physical footprint and long-run plateau are unknown.
7. **Audio:** native stream startup exists; actual guest nonzero PCM and audible speaker/headphone output still need iPhone confirmation.
8. **Performance:** earlier traces are mixed-phase/thermal and counters overlap. GPU execution time is not established by CPU service or queue waits. No matched 0.46.7 sustained gameplay run exists.
9. **60 FPS:** selection mapping exists in source; actual 60 FPS engine/display cadence has not been physically verified.
10. **Script metadata/run-loop warnings:** prior reports describe engine metadata warnings/fallbacks and a reported run-loop warning without matching backtrace. Do not “fix” by suppressing diagnostics absent reproduction.

## 9. Build, tests and CI

### Toolchain

The documented iOS workflow uses macOS 15/Xcode 26+, Rust toolchain **1.98.1**, target `aarch64-apple-ios`, Node, Python, `xcodegen`, `xcrun`, and Apple asset/signing tools. The current desktop execution environment is Windows, so the actual iOS build/package cannot be reproduced locally here.

### Full integrated CI

Use `.github/workflows/rebirth.yml` on `native-ios-prototype`. It runs entitlement and Swift settings tests, adapter and upstream Rust suites, SwiftUI previews, graphics labels/scroll/Dynamic Type UI tests, original-engine import/host initialization, compiler-free iOS build/dependency audit, iOS AOT compilation/link and IPA packaging. It requires the original game WASM input for the integration/AOT stage. Typical commands from the workflow on macOS:

```sh
rustup toolchain install 1.98.1 --profile minimal
rustup target add aarch64-apple-ios --toolchain 1.98.1
python3 tools/test_ios_entitlements.py
swiftc ios/RebirthSettings.swift tests/RebirthSettingsTests.swift -o /tmp/rebirth-settings-tests
/tmp/rebirth-settings-tests
cargo +1.98.1 test --locked --manifest-path tools/Cargo.toml --release
cargo +1.98.1 test --locked --release --bin muguet -- --nocapture
python3 tools/preview-ios.py
python3 tools/test-launcher-ui.py
```

Then follow workflow steps to validate the original engine module, run host init, AOT compile and link, and package. Do not use the unmodified upstream ZIP installer for this fork. Do not commit game WASM, archives, `web` assets, IPA, decrypted delivery artifacts or private keys.

Packaging (`tools/package-ios.py`) checks the built binary and app identity, icon catalog, ad-hoc signature/entitlements, IPA metadata and SHA-256, then encrypts private delivery. The `rebirth-private-ipa` artifact is encrypted to the user's recipient key. `tools/delivery-public.pem` is public; the private key stays outside Git. Entitlements are declared in `ios/Muguet-sideload.entitlements` and `ios/Muguet.entitlements`.

`.github/workflows/rebirth-memory-sanitizer.yml` runs a scoped macOS host AddressSanitizer/leak test for tools contracts; its own comments state it does not cover complete Wasmtime host, GPU/Metal, UIKit or device gameplay. It is not TSan or proof of race freedom.

**CI status as verified here:** only the historical package-only success `40224770057` is recorded in the 0.46.7 report; it skipped tests and packaged commit `12dedc...`. No live GitHub Actions status was queried for current HEAD `2c239b...`. Prior full UI suite failures/timeouts remain unresolved. No test/build command was run for this handoff.

## 10. Recommended priorities and acceptance

1. Review the current two tracked diffs and untracked 0.46.7 report; preserve them. Confirm they address accessibility scroll reach without weakening the test's meaningful visibility assertions.
2. Run Swift settings/unit tests and previews, then the focused Graphics UI suite on CI. Acceptance: every row name and value visible and interactable across all rows, landscape sizes and accessibility text; persistence/preset interactions pass, with no scrolling past the last row.
3. Build a candidate from the **current exact working tree** in macOS CI, identify whether dirty source is included (do not silently commit/stash it), and verify the new IPA's embedded source SHA/version/hash. Existing package cannot validate local edits.
4. Install candidate on iPhone, verify folder access, launch Story and Sandbox separately, verify visible game pixels, movement/camera, sound and save separation. Confirm installed version via launcher Diagnostics; capture current log.
5. If memory/termination investigation continues, collect device `.ips`/Jetsam and a fresh same-build log. Compare footprint, advisory headroom, Metal allocation/census, actual completed drawable timing, thermal state and session journal without summing overlapping counters. Success requires explicit OS evidence or retain “cause unknown.”
6. Only then run a matched longer gameplay test for 60 FPS, memory plateau and frame stalls. No optimization or FPS claim should be based only on PRESENT requests or host fixtures.

## 11. Data protection and Git safety

- Keep user game folder (`web` with `data/`, archives and `b/8b0b5899ed/shaders/`) in place. It is not source-control content and is intentionally excluded.
- Preserve both legacy save roots and Rebirth `Application Support/RebirthSaves/{NativeRuntimeProbe,NativeSandboxProbe}/userdata`; do not overwrite them. Persistent settings/bookmarks/logs live in app preferences/Application Support.
- Never commit `.ipa`, raw `.wasm`/`.cwasm`, `.rpf`, game-derived build outputs, decrypted delivery files, private signing/recipient key, or device logs containing sensitive paths/IDs. Git ignores these classes (`.gitignore`). Do not print or copy private keys.
- Public recipient key `tools/delivery-public.pem` is not a signing secret. Do not mistake it for the external private decryption key.
- Preserve the initial modified files and untracked report listed above. Do not reset, checkout over, stash, stage, amend or commit them without explicit direction. This handoff itself must remain uncommitted.
- Parent workspace restriction also prohibits using the moved PC port directory `C:\Users\yosnuffy\Desktop\GTA-Browser-Offline-PC` and the separate `Desktop\GTA-Browser-Offline` folder.

## 12. Evidence gaps

- No matching `.ips`/Jetsam report or symbolicated termination record was inspected.
- The named recent device log was not available in this checkout during the previous investigation; the values in the 0.46.6 report are explicitly user-transcribed.
- Current phone-installed version, current source's CI state, current checkout's gameplay, speaker output, real GPU execution timing/residency, full UI-test result and sustained 60-FPS behavior are unknown.
- Existing screenshots/ZIPs under `build/labels-evidence*` are test artifacts; they are not a signed-device validation or current IPA.

### Evidence index

- `REBIRTH.md:1-43` — fork identity, architecture, assets/saves, launcher, limits and documented build flow.
- `REBIRTH-0.46.6-STABILITY-PERFORMANCE.md:11-29,30-110,124-149` — historical memory/termination scope, changes, host measurements and limitations.
- `REBIRTH-0.46.7-GRAPHICS-LABELS.md:1-24` — label change, prior IPA provenance and test/device limitations.
- `.github/workflows/rebirth.yml:1-70` — actual integration and packaging workflow.
- `.github/workflows/rebirth-memory-sanitizer.yml:1-80` — sanitizer scope.
- `ios/RebirthLauncher.swift:43-95,113-168` — setting-row implementation, launcher structure and Graphics options.
- `ios/RebirthSettings.swift:1-100` and `src/rebirth/settings.rs` — identity, validation and configuration model.
- `src/gpu_resolve.rs:1-82`; `src/gpu.rs:3490-3517,3811-4001` — transfer semantics, presentation and memory census.
- `src/memory_budget.rs:1-110`; `src/rebirth/session.rs:1-160`; `src/rebirth/logs.rs` — pressure policy, termination journal and retained logs.
- `tools/package-ios.py:1-34` — package identity, signing, entitlement verification and hash generation.
