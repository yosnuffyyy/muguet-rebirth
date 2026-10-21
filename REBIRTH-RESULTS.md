# Project Rebirth — Native 0.46 integration milestone

## Outcome and acceptance boundary

This is a genuine, complete fork of Muguet, with its history and GPL-3.0-or-later attribution retained. It is not the old renderer renamed. The original engine has run through Muguet's real Wasmtime host, TLS initialization and constructors on macOS. A custom Swift launcher, original-folder adapter, frozen configuration, isolated save migration and disk log sharing are implemented.

**This is an integration candidate, not a verified replacement for Native 0.45 gameplay.** No physical iPhone was available. World loading, correct scenes, Story/Sandbox behavior, audible audio, full control parity, historical graphics regressions and sustainable FPS are not established by these checks. The user explicitly authorized the largest validated milestone when full completion is blocked; the remaining work is listed below.

## Provenance

- Actual GitHub fork: https://github.com/yosnuffyyy/muguet-rebirth
- Development branch: `native-ios-prototype`; fork `main` remains upstream.
- Upstream: `1b4600b052b864c78fe36180073ff406663b7e0b`, with 158 ancestor commits and an `upstream` remote.
- Integration source: `870500c8901198a09df8bbc10178c51b3f7e0c52`.
- Exact integration tree: `75d3e3274c44760a719695658b2573bcc9174712`.
- Original engine: 66,172,903 bytes; SHA256 `e65bec476e8fd8125515150c24037c9bd0d0c46530e3544f9d389ab108b5592e`.
- Original shader build: `8b0b5899ed`; no substitute engine or shader build selected.
- Rust 1.98.1 and committed dependency lockfiles; Wasmtime 49 and wgpu 30.0.1.

## Architecture and source changes

The execution path is original WebAssembly → Muguet's transformations → Cranelift build-time AOT → Wasmtime runtime on ARM64. Muguet's Rust wgpu/Metal, threading, Opus/CoreAudio, controller/touch, BC/ASTC and multiplayer implementations remain the foundation. Multiplayer is disabled by the frozen offline launch contract.

The iOS build uses `--no-default-features`, embeds the compiled original module, and has no on-device Cranelift compiler. Dependency checks reject old wasm2c, wgpu-native and compiler dependencies. Mach-O/framework and embedded-code evidence is retained in CI. The legacy C partitions, Renderer V2 and CPU Core V2 are not part of this Cargo build.

| Files | Custom behavior |
|---|---|
| `src/rebirth/settings.rs`, `ios/RebirthSettings.swift` | Typed, validated, persisted settings; presets; frozen startup values and original-engine argument mapping |
| `ios/RebirthLauncher.swift`, `ios/Launcher.swift`, `build.rs` | Native charcoal/gold library interface, prominent PLAY, scrollable categories, mode selection, original folder picker and TXT sharing |
| `src/rebirth/layout.rs`, `assets.rs`, `src/io.rs`, `src/fs.rs` | Existing assets read in place, frozen numeric-file-ID manifest, bounded archive access, case mapping and destination validation |
| `src/rebirth/abi.rs`, `src/fs.rs`, `src/host.rs` | Original 104-byte stat64 contract, short-destination failure and declared shared-memory maximum |
| `src/rebirth/saves.rs` | Byte-verified, isolated Story/Sandbox copies; originals never overwritten |
| `src/rebirth/logs.rs` | Disk-backed bounded capture and stable TXT exports, explicit capture-failure reporting |
| `src/audio.rs` | Checked PCM-ring descriptor; frozen master gain/mute applied to actual output samples; mute continues consumption |
| `src/input.rs`, `src/pad.rs` | Independent touch/controller look gains without multiplying both preferences together; upstream desktop path retained |
| `src/app.rs`, `src/config.rs`, `src/main.rs` | Freeze-before-start integration, no legacy config override on iOS, real-engine verification entry point |
| `ios/Info.plist`, `ios/project.yml`, `tools/prepare-icons.mjs`, `tools/package-ios.py` | Existing bundle identity, supplied icon, AOT packaging and integrity evidence |
| `.github/workflows/*`, `tools/*`, `tests/*` | Pinned upstream baseline, integration/Swift tests, simulator screenshots, AOT build and private encrypted delivery |

`src/patch.rs`, `src/mp.rs` and `src/touch.rs` retain upstream implementations. The only change in `src/gpu.rs` redirects the existing pipeline-recipe cache into private application storage; graphics execution remains upstream. Historical graphics fixes were not blindly copied into different code.

## Files, settings and data preservation

Select the existing `web` folder. No archive relocation, rewriting, ZIP importer or game redownload is required. The adapter pins the original manifest and shader-build directory. A single frozen manifest is used both for guest publication and numeric archive IDs, preventing a provider metadata replacement from silently reassigning file IDs during launch.

Upstream normally writes pipeline recipes beside the shader directory. The fork redirects them to `Application Support/RebirthCaches/1b4600b-v1/8b0b5899ed` so the selected folder remains read-only. A regression verifies reuse without writing into the asset directory.

Archive reads use one 64 KiB scratch buffer and an eight-file descriptor cache. Case correction occurs only after an exact-path miss. Descriptor-relative traversal rejects symlinks; ambiguous case matches, truncation, missing files, cancellation and failed destinations remain errors. This is not a claim that every File Provider implementation or hostile concurrent directory replacement has been validated on iOS.

Legacy `Application Support/NativeRuntimeProbe` and `NativeSandboxProbe` files are copied into separate `RebirthSaves/<namespace>/userdata` roots. Bytes are reread and verified before activation. Migration is capped at 256 MiB, 10,000 entries and 32 directory levels. Incomplete staging stays inactive for inspection. This changes filesystem placement, not save payload formats. Actual engine save compatibility remains pending.

Supported launcher values include 50/70/100% scaling (640×360 → 320×180 / 448×252 / 640×360), explicit base dimensions, six presets, original texture/FXAA/shadow/reflection/water/particle/grass/postFX/SSAO/filtering arguments, ambient density, master gain/mute and independent look sensitivities. Only validated divisor caps 15/20/30 FPS are selectable. Unsupported MSAA, higher/uncapped rates and advanced settings are visibly unavailable. Argument delivery is verified; effective engine settings after world initialization are not yet read back.

Story uses mode 1 and Story-only `-noautoload`. Sandbox uses mode 2 and the existing offline player-coordinate argument, without Story's `-noautoload`. The original input start-mode publication receives the frozen choice. These are source/contract assertions, not evidence of free-roam gameplay in this new fork.

All 45 existing IPA artifacts were checked by size and SHA256 without modification. Native 0.45 remains the rollback build: `cbe19d8ab8d9073071c4eff426189ea1d5f6898600435e81daf6cc2d6384588f`. Protected Desktop folders and the deployed legacy main branch were not used or modified.

## Verification

The unmodified upstream baseline passed macOS tests and an ARM64 iOS link: https://github.com/yosnuffyyy/muguet-rebirth/actions/runs/39678552818

The integrated original-module check completed all 94 typed imports, real host instantiation, TLS and engine constructors. Shared memory is wasm32 with i64-pointer host ABI, 8,192 minimum and 65,536 maximum pages. Constructor execution grew the heap to 614 MiB; this is heap commitment, not process footprint. World/main/GPU execution is intentionally outside that host check.

The first integrated run failed on a test-crate rename and Swift Button syntax; both were corrected. Later integration and embedded-AOT IPA builds passed. Final-source CI completed successfully, including the embedded original-module AOT package. Results and artifact identity are recorded below.

Local Windows verification: nine portable adapter tests passed. Unix archive tests are explicitly excluded on Windows and run on macOS, not counted as Windows passes. Tests cover original stat64 bytes/canaries, manifest identities, invalid paths/sizes, layout, settings validation/mode changes, isolated save migration, log rotation and stable export snapshots.

macOS additionally exercises descriptor-bound reads, the eight-file cache, cancellation, expected EOF, truncation, symlink rejection, file replacement, physical case mapping, frozen manifest replacement and original SharedMemory stat syscall bounds. Swift assertions cover preset synchronization, migration, persistence, invalid caps and corrupt settings. Actual SwiftUI simulator captures validate layout only; their test executable deliberately cannot start a game.

The earlier ABI audit noted that the original Opus frame function has five parameters while the upstream patch forwards a sixth zero-initialized local. Wasm validation succeeds, but decoder-state/FEC and original PCM equivalence still need a real packet fixture; this is not a confirmed cause of silent audio.

Final macOS suites: **9 adapter unit tests, 4 Unix archive integration tests, 13 complete-host unit tests, and 20 Swift assertions passed**. The host suite includes the shared adapter cases; these counts must not be summed as unique tests. Seven actual SwiftUI pages were captured on an iPhone 16 Pro simulator running iOS 26.2. Screens were inspected, and the off-screen footer/banner/sidebar defects found during the first capture were corrected. Scroll gestures and production engine launch were not exercised by the preview harness.

ASan, UBSan, TSan, real GPU pixel equivalence, physical-controller events and physical speaker output have not been validated in this milestone. Old-runtime graphics tests are not reported as passes for Muguet.

## Performance evidence and limits

On the final macOS CI run (`870500c`), original-module transformation took 703.421 ms and transformation plus compilation/loading took 19.317 s. These are build/host startup measurements, not iPhone frame time. They have no comparable legacy workload and do not establish an improvement.

Bounded costs established from implementation/tests: 64 KiB archive scratch; eight cached archive descriptors; maximum 8 MiB manifest; 16 KiB settings input; 256 MiB rolling log retention and two export files; fixed 32 KiB log-copy scratch. These bounds are not total application memory. No sustained CPU/GPU, thermal, FPS or input-latency comparison is available for this fork. No 30 FPS claim is made.

## Remaining work and next device check

1. Sign/install with the same bundle identity and appropriate entitlements, without deleting the legacy app data. Retain Native 0.45 separately for rollback. Verify the retained folder bookmark and settings migration.
2. Select the existing `web` folder and start Low / 640×360 / 100% / 30 FPS. Confirm logs identify Muguet/Wasmtime/wgpu30 and the exact original shader build. Export the full TXT report if startup fails.
3. Independently test Story and Sandbox after fresh launches. Verify actual world, controllable player, camera, vehicles, collisions, save creation and separate progress; never infer this from mode getters.
4. Test audible dialogue/weapons/vehicles, DualSense and multitouch, focus/interruption behavior, pause menus, known shadow/SRV cases and the prior vehicle-impact alpha-to-coverage case. Upstream presence is not proof of compatibility.
5. Record comparable marked gameplay at identical quality and resolution, then longer driving/transition sessions with memory and thermal observations. This is needed before selecting new performance changes.

Still unimplemented or incomplete: full legacy custom touch-layout/deadzone/invert and automatic-switching parity; fullscreen preference parity; advanced audio buses; live effective-setting readback; graceful return-to-launcher/relaunch parity; complete historical renderer regression adaptation; recovery UX for interrupted save migration. Upstream currently owns process-exit behavior. The selected root must contain the real archives; they are not present in the approved local minimal-engine fixture, so full local world loading could not be run.

## Delivery

Final verification run: https://github.com/yosnuffyyy/muguet-rebirth/actions/runs/39685241424

**PASS — Native 0.46.0, build 46**, code revision `870500c8901198a09df8bbc10178c51b3f7e0c52`.

- IPA: `GTA-Rebirth-0.46.0-870500c89011-unsigned.ipa`
- Size: **63,132,234 bytes**
- SHA256: `af433956d781507e66a59e81e26b582cc544c97877ef6c5605c737e09f3b523b`
- Mach-O UUID: `B0198EED-9808-3EC4-92DF-A53FA8657088`
- ARM64 executable; original compiled game in a **158,433,280-byte read/execute-only `__GAME` segment**.
- Existing bundle ID: `io.github.yosnuffyyy.gtanativeprobe`; display name: GTA V (Test Build).
- ZIP integrity, authenticated decryption, digest, embedded code, framework dependencies, bundle metadata, icon asset catalog and ad-hoc signature checks passed. User signing is still required for installation.
- Public evidence artifact: `12386143610`; encrypted IPA artifact: `12385958819`. Both download archive digests were independently verified.
- Local delivery: `outputs/GTA-Native-Prototype/GTA-Rebirth-0.46.0-870500c89011-unsigned.ipa`.
- Local evidence/screenshots: `outputs/GTA-Native-Prototype/Rebirth-0.46-Verification/`. Simulator screenshots retain the simulator's native capture orientation; they are interface evidence, not gameplay.
- Physical-device verification: **pending**.


Raw WASM, compiled game objects, archives, shader assets and usable game-derived IPAs are not published in the public repository or public release artifacts. CI encrypts the package to a recipient key whose private half remains local and ignored by Git. The decrypted candidate is delivered locally after authenticated decryption and hash/ZIP/Mach-O inspection. Public evidence contains build results and metadata.

## Compact report for ChatGPT

Project Rebirth is a real full GitHub fork of c22dev/muguet at upstream `1b4600b052b864c78fe36180073ff406663b7e0b`, not renamed wasm2c. Source `870500c8901198a09df8bbc10178c51b3f7e0c52` uses Wasmtime 49, build-time Cranelift AOT and Rust wgpu 30/Metal. Original module SHA256 is `e65bec476e8fd8125515150c24037c9bd0d0c46530e3544f9d389ab108b5592e`, 66,172,903 bytes. The real original host/TLS/constructors passed on macOS with 94 typed imports and 614 MiB heap commitment; world and GPU rendering were not started by that check. iOS has no runtime compiler or legacy C/wgpu-native stack. Custom Swift launcher, frozen validated settings, read-in-place web-folder adapter, 64 KiB/eight-descriptor archive reader, corrected 104-byte stat64, isolated byte-verified save copies, bounded 256 MiB disk logs, PCM gain/mute and independent input gains are implemented. Existing 45 IPAs were preserved. Physical Story/Sandbox, scenes, audio, controls, save compatibility, historical graphics pixel tests and sustained performance remain unverified; several legacy control/UI preferences remain unported. Final CI passed with 9 adapter unit cases, 4 Unix integration cases, 13 host unit cases (including shared tests) and 20 Swift assertions. Packaged ARM64 version 0.46.0/build 46 has UUID B0198EED-9808-3EC4-92DF-A53FA8657088, IPA size 63132234 bytes and SHA256 af433956d781507e66a59e81e26b582cc544c97877ef6c5605c737e09f3b523b. No iPhone FPS gain or full migration completion is claimed. Next priority is actual original-asset world startup, then compatibility regressions and equal-workload profiling.
