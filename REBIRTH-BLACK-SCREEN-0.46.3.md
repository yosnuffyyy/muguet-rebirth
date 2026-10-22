# Rebirth 0.46.3 — black-screen repair

## Evidence and confirmed cause

Baseline checkout: `6676c82a30ed46722b46bd2702f9857dff85fafd` (documentation after executable `8cb11d7aaac06428cfdc1972d58b4be491db19f7`, 0.46.2/build 48). Runtime remains original engine e65bec476e8fd8125515150c24037c9bd0d0c46530e3544f9d389ab108b5592e, Wasmtime 49, wgpu 30.0.1, Metal. No wasm2c fallback.

The supplied `Blackrebirth-1792626002794032000-0.txt` is 9,277,363 bytes, SHA256 `74078e66df4d6e01f437eaaf2a146abf7365cf4691d4dd456af2de21cd51f4d3`. It contains several sessions, not one continuous run. A source-only summary is in `evidence/BLACK-SCREEN-DEVICE-SUMMARY.json`; the private raw log is not uploaded.

Opcode 36 unconditionally opened a render pass with `resolve_target`, including when both textures had one sample. The recorded errors explicitly occur both in the encoder and Queue::submit. Invalid command buffers cannot establish successful rendering even when surface-present calls continue. This is a confirmed defect with a direct mechanism for losing scene rendering. It is not yet proof that it was the only cause of black physical output.

The latest detailed session's 6,340 validation errors and 3,465 skipped draws are pre-fix device evidence. Earlier sessions have different counts. Logs also show genuine script activity and nonzero audio callback samples; user reports controls and audible gameplay. Those observations do not establish rendered game pixels.

## Implemented changes

- `src/gpu_resolve.rs`, called by the actual `src/gpu.rs` opcode-36 handler: single-sample transfers copy the selected mip/layer; supported MSAA resolves use a render pass. Original subresource numbering includes dropped mips; invalid/dropped selections fail explicitly, without clamping. Extents, dimensions, compatible native formats, usages, destination samples and resolve capability are checked before recording the operation. Identical single-sample subresources are an identity operation. Unsupported operations retain source/destination IDs and subresources in fatal diagnostics.
- Pending texture conversions and deferred clears remain ordered before transfers. The MSAA source is stored, not discarded, so later reads/resolves retain its contents. Encoder ownership and existing resource-use tracking are preserved.
- Required draw pipelines no longer cause dropped draws when the old eight-millisecond compilation budget expires. Background compilation/prewarming remains; the consuming draw waits for its exact result. Disconnection, compilation failure and a 60-second stalled wait are explicit failures. This can produce cold compilation pauses; it is a correctness repair, not an FPS optimization.
- Unbound shader-output messages are clarified. The pinned wgpu-core explicitly allows fragment outputs without corresponding color attachments (`vendor/wgpu-core/src/device/resource.rs`). No extra targets, shader removal, or speculative MRT remapping is introduced.
- Presentation prefers a supported opaque surface-compositing mode. Texture alpha bytes are preserved. The launcher finishes dismissal before starting the engine, avoiding an overlapping opaque launch sheet during startup.

The native format checks cannot reconstruct an omitted D3D resolve-format parameter for unsupported typeless reinterpretations. Those combinations fail with diagnostics rather than inventing a conversion. D3D11's formal ResolveSubresource contract requires an MSAA source; the 1x copy is the required compatibility interpretation of this engine protocol, not a claim that D3D11 defines single-sample resolves.

## Optional display diagnosis

Diagnostics → **Display diagnosis (temporary magenta test)** is off by default and applies on the next clean launch. The first 30 presentation attempts show magenta through the real surface, then normal game blitting resumes. This is explicitly marked as test imagery.

At presentation indices 30, 60, 120, 240, 480, 900, 1200 and 1800, asynchronously read five diagonal pixels from the actual game backbuffer and actual surface after the final blit. Each readback is 1,280 bytes; eight pairs total 20,480 bytes across the session, with no full-frame captures or added GPU completion waits. Unsupported surface COPY_SRC/format paths report UNKNOWN. Samples include raw color/alpha bytes in each texture's native channel order, nonblack count and changes from prior ordered samples. Black samples do not prove the entire frame is black. Nonblack samples do not prove drawable display completion.

A one-time UIKit inspection (at most 256 layers, including nested Metal sublayers created by raw-window-metal) records CAMetalLayer visibility, effective view alpha, frame, window attachment, layer opacity and remaining presented controller. It does not claim screenshot or display proof. No polling UI traversal or gameplay UI animation loop is added.

## Validation status

Final [CI run 39697049904](https://github.com/yosnuffyyy/muguet-rebirth/actions/runs/39697049904) passed on executable revision `6768a3c4db0df000d8b6abb6cb18d29e46fba9bf`.

| Check | Verified result / provenance |
|---|---|
| Production transfer + blit pixel fixture | Apple Paravirtual device, Metal, macOS CI: 12 combinations of 640x360 / 1280x720, RGBA8 / BGRA8, 1x / 2x / 4x. Zero validation errors. Exact byte comparisons for copy/final blit, per-sample MSAA averaging, repeated resolve source retention, array/mip isolation, self identity and invalid range/format/extent/sample rejection. |
| Unbound MRT outputs | Fragment locations 0/1/2 with only target 0: expected green pixels and no validation errors. No added attachments or output-removal shader transform. |
| Full host tests | 20 passed, 0 failed/ignored; includes the new GPU fixture and bounded-probe/subresource tests. |
| Adapter / archive tests | 13 adapter unit tests and 4 asset-reader integration tests passed. Some unit coverage overlaps the full host suite; these are not additive distinct tests. |
| Settings / UI | 20 Swift settings, migration, preset and persistence assertions passed. Actual Swift launcher Simulator previews captured; Diagnostics toggle preview inspected. This is not the game runtime. |
| Original engine | Actual pinned original module loaded through Wasmtime; 94 typed imports, initialization and signed input ABI checks passed, alternating both modes and both resolutions, repeated publications, held/released keys and invalid pointers. This does not prove scene spawn. |
| ARM64 / package | Compiler-free iOS dependency check, original iOS AOT generation and link, bundle identity, codesign entitlements and IPA packaging passed. Local download digests, authenticated decryption, ZIP CRC, ARM64 Mach-O and version/bundle metadata checked. No web directory or RPF game archives in the IPA. |

The GPU fixture completed within the 1.27-second full-host test run on the CI virtualized Apple Metal device. This is correctness evidence, not an A18 Pro gameplay benchmark. No new ASan/TSan run or physical iPhone session was available. Full-game visible output, physics, controls, audible output, compile-pause behavior, actual display completion and sustained stability remain device checks. No FPS improvement or crash-free gameplay claim.

## Delivery

- Version **0.46.3**, build **49**; display name GTA V (Test Build).
- Executable source: `6768a3c4db0df000d8b6abb6cb18d29e46fba9bf`, branch `native-ios-prototype` in `yosnuffyyy/muguet-rebirth`.
- IPA: `GTA-Rebirth-0.46.3-6768a3c4db0d-unsigned.ipa`, **63,158,232 bytes**, in `outputs/GTA-Native-Prototype`.
- SHA256: `e4e453469031171a3bdb3253aee385553a41308a07c036f9553bf63dd6680dde`.
- Mach-O UUID: `04091096-CD94-3CBA-AAAD-399229994E7E` (ARM64).
- Bundle identity unchanged: `io.github.yosnuffyyy.gtanativeprobe`.
- Ad-hoc packaged; requires AltStore/user signing. Existing working IPAs retained. Public CI delivery remains encrypted under the established recipient-key workflow; no game assets or raw device log published.
- Machine-readable evidence: `evidence/REBIRTH-0.46.3-PACKAGE.json`. Documentation commits after this executable revision do not change the tested binary.

## Next iPhone test

1. Install the new IPA over the same bundle identity; retain the selected web folder and both save namespaces.
2. Enable Display diagnosis, launch Story at 640x360 with the current no-MSAA profile. Report whether magenta is visible and whether genuine game imagery follows. Export the TXT report after at least 60 seconds, or immediately after an error.
3. Repeat Sandbox at 1280x720. Move, look, enter a vehicle, and verify audio. Do not treat the color test as game rendering.
4. Check opcode-36 path records, validation counts, before/after pixel samples, and UIKit visibility. A nonblack game backbuffer plus black post-blit output isolates composition; nonblack post-blit samples plus no visible color test points toward presentation/view visibility. All-black game samples require further game-pass inspection.
5. Turn diagnosis off for normal use. MSAA capability tests in CI do not add an unsupported launcher MSAA setting; device MSAA gameplay remains unverified unless a supported engine configuration is available.

References: [D3D11 ResolveSubresource](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-resolvesubresource), [WebGPU specification](https://gpuweb.github.io/gpuweb/). Implementation checks use the pinned local wgpu-core source.
