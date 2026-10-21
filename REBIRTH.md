# Project Rebirth — GTA V (Test Build)

A genuine fork of [c22dev/muguet](https://github.com/c22dev/muguet), preserving its complete source, history, GPL-3.0-or-later license and Rust runtime. Development is on **native-ios-prototype**. Fork main remains the pinned upstream baseline.

**Current milestone: original-module host initialization and a customized launcher. Physical-iPhone gameplay in this fork is not yet verified.** Native 0.45 remains the independent rollback release. Its performance and compatibility results are not results for Rebirth.

## Architecture

Original WebAssembly → Muguet transformations → Wasmtime 49 / Cranelift build-time AOT → native ARM64. Graphics are Muguet's Rust wgpu 30 / Metal. iOS builds use --no-default-features and embed compiled code in the signed executable; no on-device compiler is enabled. The old wasm2c engine, 128 C partitions, wgpu-native 29, Renderer V2 and CPU Core V2 are not linked.

- Upstream: 1b4600b052b864c78fe36180073ff406663b7e0b (158 commits).
- Original engine SHA256: e65bec476e8fd8125515150c24037c9bd0d0c46530e3544f9d389ab108b5592e (66,172,903 bytes).
- Original shader build: 8b0b5899ed.
- Legacy executable: 7030977aee7f5e5d87a206931d123d9901f793ca, release native-0.45.
- Legacy IPA SHA256: cbe19d8ab8d9073071c4eff426189ea1d5f6898600435e81daf6cc2d6384588f.

## Existing files and saves

Select the existing **web** folder in Files. Rebirth reads data/manifest.json, archives and b/8b0b5899ed/shaders/ in place. There is no ZIP import in the customized launcher. It holds the security-scoped bookmark throughout the session. Archives use one 64 KiB scratch buffer and at most eight open descriptors; symlink traversal and ambiguous case matches are rejected.

Legacy NativeRuntimeProbe and NativeSandboxProbe directories stay untouched. At first use, their opaque files are copied and byte-verified into Application Support/RebirthSaves/<namespace>/userdata. This maps the old host's /userdata mount to Muguet's filesystem layout. Subsequent Rebirth saves never replace legacy saves. No payload format conversion is claimed. Migration is bounded to 256 MiB, 10,000 entries and 32 directory levels. An incomplete .pending migration is rejected and retained for investigation; it is never silently activated.

## Launcher and settings

The native Swift library launcher ports the charcoal/gold composition, prominent PLAY button, Story/Sandbox selection and category navigation. Swift preferences are validated again by the Rust launch contract and frozen before the engine starts. Original supported values become explicit engine arguments; diagnostics distinguish requested overrides from unverified effective gameplay.

Available: six presets, base resolution and 50/70/100% scale, 15/20/30 divisor caps, original texture/FXAA/shadow/reflection/water/particle/grass/postFX/SSAO/filtering controls, ambient density, master volume/mute, look sensitivity and upstream frame statistics. Muting drains the PCM ring. Unsupported controls remain unavailable rather than becoming placebo settings.

Full custom touch layout, deadzone/invert preferences, controller/touch switching parity, fullscreen preference parity, advanced audio buses, live effective-setting readback and historical renderer pixel regressions remain integration work. Upstream controls/audio/graphics are present, but this is not a claim of device parity.

## Diagnostics

Diagnostics → **Share TXT Report** exports a stable snapshot. Latest logs are retained across launches in eight 32 MiB disk chunks (256 MiB total). Two generated exports are retained. Capture does not retain the complete log in RAM. The export explicitly reports a storage failure. Exported command counts or audio callbacks do not prove correct scenes or audible sound.

## Build and validation

Use .github/workflows/upstream-baseline.yml for the untouched pinned baseline and .github/workflows/rebirth.yml for the integrated fork. Dependencies are pinned in Cargo.lock; Rust is 1.98.1. The integration tests the original module's real imports, TLS and constructors before iOS AOT compilation. Constructor success does not start the world or prove gameplay.

For a local Apple Silicon Mac, provide your existing matching module to tools/original-engine.mjs with its output path and source path, then follow the exact tested workflow. Do not use the upstream ZIP installer for the customized app. tools/package-ios.py preserves bundle ID io.github.yosnuffyyy.gtanativeprobe, display name GTA V (Test Build) and the user-provided icon. Installable builds still require the user's normal signing procedure and entitlement support; identity and data retention must be checked on-device.

Game archives, raw WASM, compiled game objects and usable game-derived IPAs are not published here. CI delivers a build encrypted to the user's local recipient key; the private key stays outside Git. Local packaging also leaves the unsigned/ad-hoc IPA on the authorized build machine. Public CI evidence contains tests and metadata, not game assets. Do not commit private keys or decoded artifacts.

## Upstream maintenance and attribution

The original README retains upstream credits and documentation; its installer/ZIP workflow describes unmodified upstream. Keep upstream pointed at https://github.com/c22dev/muguet.git. Custom modules live in src/rebirth, ios/Rebirth* and tools/. Original GPL notices and LICENSE are retained. See REBIRTH-RESULTS.md for verified results and remaining limitations.
