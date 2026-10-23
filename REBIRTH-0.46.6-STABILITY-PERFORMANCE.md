# Project Rebirth 0.46.6 — lifetime, scheduling and termination evidence

## Identity and scope

Baseline: Rebirth **0.46.5 / build 51**, executable `5ce6cf698975a628057dee786297fc65a653fb79`, ARM64 UUID `2567C054-665B-389A-982E-7621BF329C58`. The initial checkout was its documentation descendant `f6c134498b95127f49a588d03bee0ca0b78832a2`; no newer runtime changes were discarded. The baseline IPA remains preserved.

Candidate: **0.46.6 / build 52**, executable `e8c7f7e8a66621bfa8e73bdf4b1c6c48d2087153`, genuine Muguet / Wasmtime 49 / wgpu 30 / Metal. [Integration run 40015267583 passed](https://github.com/yosnuffyyy/muguet-rebirth/actions/runs/40015267583); downloaded IPA identity, hash, archive and ARM64 checks passed. Physical-device validation remains pending.

The original engine remains SHA256 `e65bec476e8fd8125515150c24037c9bd0d0c46530e3544f9d389ab108b5592e`, shader build `8b0b5899ed`. There is no engine transformation, quality reduction, frame-cap change, save migration change, renderer replacement or audio rewrite in this update. The prior 0.46.5 uniform staging, upload borrowing, filesystem scratch, streamed warmup and memory-growth fixes are retained, not counted as new work.

## Termination: evidence and confidence

The requested `Israelrebirth-1792783711571988000-0.txt` was not present in accessible Downloads or the attachment directories during this work. Its two most recent populated sessions could not be independently analyzed. The following are **user-transcribed physical-device observations**, not values extracted from that missing file:

| Counter | Exact supplied value | Interpretation |
|---|---:|---|
| Process footprint | 3,482,259,960 bytes / 3,320.942 MiB | OS footprint; not sum of other rows |
| Available memory | 57,732,616 bytes / 55.058 MiB | Severe reported low headroom; earlier session reportedly about 21 MB |
| Metal allocated size | 1,681,276,928 bytes / 1,603.391 MiB | Driver allocation accounting, not GPU residency |
| Texture estimate | 1,036,797,532 bytes / 988.767 MiB | Heartbeat combines engine textures and proxies; excludes driver padding |
| Buffer capacity | 364,577,512 bytes / 347.688 MiB | Engine GPU buffers and counter storage |
| Live objects | 58,078 | Includes all protocol object kinds, not only textures/buffers |
| Reported cadence | about 60 → 36.9 per second | Baseline source counts PRESENT requests, not completed displayed frames |
| Freeze reports | 315–634 ms | Engine wait samples; not attributed GPU duration |

These counters overlap and may be sampled at different boundaries. They are not additive. The missing raw log also prevents verifying settings, elapsed time, pressure sequences and phase equivalence.

**Confirmed:** supplied headroom is critically low; the source has stale cache-ownership defects described below. **Plausible but unconfirmed:** OS memory termination. **UNKNOWN:** Jetsam versus EXC_RESOURCE, native crash, Wasmtime trap, GPU loss, watchdog, force-close or incomplete export. No matching iOS `.ips`/Jetsam record was available to symbolicate. The older NativeProbe 0.36 CPU-resource report is a different executable and is not termination evidence for Rebirth.

## Implemented fixes

### Streamed GPU resources retire their dependent caches

`src/gpu.rs`, `src/gpu_lifetime.rs` and associated tests.

The old bind-group cache used numeric resource/view/sampler IDs but did not invalidate those entries on destruction or ID replacement. A separate `in_bg` flag was written but never consumed. Derived nearest samplers likewise survived sampler-ID reuse. View descriptors could retain a native texture view after the resource at that ID changed. This permits stale binding reuse and retains obsolete proxy/counter ownership until broad cache clearing. It does **not** prove that all 58,078 live engine objects or all reported texture memory were leaked.

A reverse dependency index now removes only groups referring to a destroyed/replaced resource, view or sampler. Source-resource replacement clears descriptor-owned native views; sampler replacement clears its derived sampler. `set_obj` retires an occupied ID before publishing the replacement. BC accounting and query registration occur after that publication. Immutable uniform snapshots are not invalidated just because their source constant buffer changes.

The index shares one key allocation through `Rc`, creates dependency metadata only on misses and removes empty reverse-owner sets. Existing global cache limits remain. Streaming destruction visits affected groups rather than scanning the entire cache. Extra owner vectors/reverse edges cost host memory and miss-time work; measured fixture tradeoffs are reported below. No cache index or telemetry is presented as free.

Retirement drops cache handles. Encoded/submitted wgpu work keeps its own resource references; it does not force-free active GPU storage or add a completion wait. Existing base-resource destruction/submission ordering remains. In particular, this fixes ownership of derived depth proxies and counter buffers, which previously survived through cached bind groups. Driver release timing is still governed by completion and native ownership.

New logs report retired groups, cumulative logical proxy/counter payload, cache owners/edges and descriptor owners. **Logical retired bytes are not measured physical bytes freed.** Pressure requests now explicitly report when no cold uniform tail is eligible, rather than leaving a request count that could be mistaken for successful reclamation.

### Defer the frame-slot fence to uniform writes and command submission

Previously, `Gpu::present()` selected the next of three uniform regions and immediately blocked on that region's prior submission, before decoding/preparing the next commands. This serializes CPU preparation behind a GPU fence even though packing the independent CPU stage does not overwrite GPU bytes.

The same exact submission-index fence is now checked inside `flush()`, before `flush_uniforms()` and `queue.submit()`. CPU decoding, descriptor construction and uniform packing can proceed first. Ordinary resource `queue.write_*` calls can still stage bytes earlier; those copies execute with submission. The three GPU slots, region offsets, submission ordering and maximum admitted GPU work remain unchanged. A failed required wait becomes an explicit fatal error rather than allowing reuse after an ignored wait result.

The first subsequent flush may still happen early. This change exposes safe CPU/GPU overlap; it does not eliminate required GPU work or guarantee that a particular 634 ms freeze disappears. A saturated GPU, drawable acquisition, required pipeline compilation, engine dependencies or pressure can still stall. Earlier preparation can also overlap transient staging/resource allocation with retiring GPU work; unchanged slot capacity alone does not prove unchanged peak process footprint. Source review and pixel tests can establish safety; only a matched device trace can establish throughput and peak-memory effects.

The unused `Vec` of pending-map indices, allocated/scanned on each submission and immediately discarded, was removed. No map, query or completion callback was removed.

### Release unused manifest lookup maps

`src/rebirth/assets.rs`.

The asynchronous archive worker addresses entries by original numeric manifest ID. After validating the complete manifest, it now retains only that ordered entry vector, releasing the unused exact-name and case-folded lookup maps. Startup validation, ambiguous-path rejection, immutable manifest bytes, numeric IDs and archive ownership are unchanged. One startup line reports actual retained/released path capacities; it does not guess OS footprint savings.

### Persist process-outcome evidence and bound repetitive logging

`src/rebirth/session.rs`, `logs.rs`, `repeated_warning.rs`, `src/host.rs`, `src/app.rs`.

A journal limited to 8 KiB plus one temporary file records session/build/source identity, wall-clock timestamps, launch request, explicit user stop, engine exit code, first fatal and Rust panic. Same-directory replacement follows a complete synced write. An interrupted temporary write leaves the prior complete record. Panics use a nonblocking lock attempt. There are no unsafe signal handlers, new heartbeat threads or per-frame journal writes.

On relaunch, the prior outcome is read, the new record is persisted, then the prior summary is copied into the ordinary exportable log. An abrupt exit in that narrow intervening interval can prevent the prior summary from being exported; this is not an OS crash recorder. An unfinished record is explicitly “no recorded clean exit; cause unconfirmed,” including the possibility of user force-close. An explicit stop is not described as proof that engine shutdown finished. Later exit requests cannot overwrite an earlier fatal/panic. Existing periodic memory/renderer breadcrumbs and GPU-loss diagnostics remain.

Only the exact historical ClothInstanc unlock-warning line is aggregated: first occurrence, totals when another matching warning arrives after at least five seconds, and final/export totals remain. There is no periodic warning timer. New wording/context, assertions and fatal messages pass through unchanged. The older available log contained 711 exact occurrences; new-session frequency and actual I/O cost are unknown. This is diagnostic overhead reduction, **not a claimed fix to cloth buffer behavior**.

## New bounded measurements

`src/gpu_timing.rs`, `src/gpu.rs`, `src/startup.rs`.

Fixed 256-sample histories use **21,312 bytes** in the tested host layout. Each completed PRESENT records category sums and counts; mean/p50/p95/p99/max and lifetime summaries are computed at reporting time for:

- PRESENT interval and command-handler wall time;
- frame-slot fence wait, submission API, drawable acquisition and presentation API;
- device polling, readback handling and required-pipeline waiting;
- time idle waiting for the producer.

Categories are nested and **must not be added**. Command events count timed segments, not protocol commands. Command wall excludes service work between command batches and periodic diagnostic formatting. PRESENT boundaries include map housekeeping and pressure processing. Readback timing covers the handler, not the complete asynchronous request-to-engine result interval. Slot wait includes its device polling. Statistics are formatted only at the existing ten-second reporting boundary.

Queue records include unchanged slot count, slot-wait count, sampled command-ring occupancy high-water mark and pending map/query buffer count/bytes. Occupancy is not producer-blocked time. The heartbeat samples an explicit GPU-service phase, including slot fence, polling, pipeline work, drawable acquisition and suspension; the last phase does not identify a crash cause.

The ambiguous old log `fps:` label is changed to **PRESENT requests/s**, explicitly not displayed FPS. The existing on-screen counter also derives from PRESENT requests and must be treated as approximate request cadence. Genuine GPU execution time, per-core active CPU samples, actual display-completion percentiles, driver resource residency and end-to-end input-to-display latency remain UNKNOWN. The pinned backend supports [targeted submission waits](https://docs.rs/wgpu/30.0.1/wgpu/struct.Device.html#method.poll); its Metal submit/present paths already have autorelease pools. Blanket new autorelease pools were not justified.

## Measurements and tests

**Integrated macOS/Metal CI passed:** 65 complete host tests, 41 portable contract tests, five Unix archive tests, 40 Swift settings assertions and two entitlement tests. Rust suites overlap and must not be summed as unique tests. All seven Swift launcher simulator previews completed; Home and Diagnostics were visually inspected and show 0.46.6/build 52. These are actual Swift layout previews without the production Rust runtime linked, not gameplay tests.

Original-module verification passed actual Wasmtime instantiation, 94 typed imports, TLS/constructors, input hooks, held/released keys, invalid pointers, and repeated publication for two modes and two resolutions. World/main/GPU gameplay was not started by this fixture. The iOS build excluded the compiler backend and legacy wasm2c/wgpu-native dependencies, linked the original AOT module, and passed packaging/signature checks. The expected `should_panic` staging bounds test passed; its printed panic is not a failed test.

Windows release contract suite: 41 passed; Unix filesystem integration is not executed on Windows. One million telemetry accumulation calls took 1.0127 ms on the local Windows host and 2.977625 ms on macOS CI, excluding clock reads, sorting and formatting. These are optimized synthetic tests, not a bound on complete iPhone instrumentation overhead. The ASan-instrumented equivalent took 55.616084 ms and is not comparable production performance.

[AddressSanitizer run 40015584798 passed](https://github.com/yosnuffyyy/muguet-rebirth/actions/runs/40015584798): 41 contract tests plus five Unix archive tests, macOS ARM64, Rust 1.98.1, leak detection and stack-use-after-return detection enabled. No sanitizer errors reported. Revision `45746ea28a8fd40b101badaa432aa841a8d05dac` changes only the standalone sanitizer workflow after executable source `e8c7f7e8a66621bfa8e73bdf4b1c6c48d2087153`. Coverage excludes uninstrumented distributed Rust std, host build scripts/procedural macros, complete Wasmtime execution, GPU/Metal, UIKit and iPhone gameplay. No whole-runtime ASan, TSan or UBSan pass is claimed. Sanitizer artifact ZIP SHA256: `d0756e3588c0aeff6d9395e59d58bbc603fdfbc6f824a35d27d7dd2b33147fb5`.

| Comparable fixture | Baseline | Candidate | Meaning |
|---|---:|---:|---|
| 20,000-entry manifest path storage | 2,340,000 bytes across entry paths and lookup keys | 780,000 entry-path bytes | 1,560,000 duplicate bytes plus 40,000 map entries released; allocator/footprint not measured |
| Streaming cache churn: 4,096 resources, 128 live, four groups/resource | 16,384 groups / 64 MiB modeled payload / 23.383 ms host wall | 512 groups / 2 MiB modeled payload / 13.952 ms host wall | 62 MiB less synthetic retained ownership; not actual city resource sizes or physical RAM |
| Two obsolete 4 MiB depth proxies | Old groups retain proxy ownership until broad clearing | Cache owners released at destruction; 8 MiB logical retirement | Correct pixels with work in flight; driver residency not measured |
| GPU frame regions | 3 | 3 | Same completion fence and GPU memory bound |
| Device steady/peak footprint after changes | UNKNOWN | UNKNOWN | No new iPhone trace |
| Device FPS/freezes/headroom after changes | UNKNOWN | UNKNOWN | 400–600 MiB target and freeze elimination unverified |

Production-path Metal fixtures on CI's **Apple Paravirtual Metal adapter**, not A18 Pro, passed texture/view ID recycling, unchanged-binding hits, depth-proxy retirement with work in flight, UAV data/counter generations and nearest-sampler replacement. The production memory fixture passed fourteen draw pixels across all three uniform slots, deferred-fence ordering, cold-tail reuse, exact packed constants and upload bytes with zero validation errors. Existing resolve, shader-output/final-blit, settings, save isolation, input, filesystem and audio-ring regressions passed. Host original-module initialization is not proof of playable Story/Sandbox, and audio-ring tests do not establish audible game playback.

The cache-churn timing is one sequential synthetic comparison inside a concurrently executing test suite. Fixture checks, allocations, cache warmth and allocator scheduling are included. The reverse index adds metadata; its purpose is exact dependency retirement, and neither the 40.3% fixture wall reduction nor 96.875% modeled payload reduction can be extrapolated to game frame time or footprint. Earlier 0.46.5 staging/UAV/filesystem fixtures were rerun successfully, but their savings are not new improvements in this release.

## Delivered IPA

- Version **0.46.6**, build **52**, source **`e8c7f7e8a66621bfa8e73bdf4b1c6c48d2087153`**.
- `GTA-Rebirth-0.46.6-e8c7f7e8a666-unsigned.ipa`, **63,210,014 bytes**.
- SHA256 **`0bb2f67fd7517ed271d01cebf7557284929953d0674965809ac08cdd60e6b24d`**.
- ARM64 Mach-O UUID **`66A043C1-30A7-3FD4-AF2A-CD4B65C0AE31`**.
- Local artifact: `outputs/GTA-Native-Prototype/GTA-Rebirth-0.46.6-e8c7f7e8a666-unsigned.ipa`, relative to the native workspace root.
- Bundle remains `io.github.yosnuffyyy.gtanativeprobe`; display name remains `GTA V (Test Build)`. The prior IPA is preserved.
- CI verified the ad-hoc signature and both requested memory entitlements. Installation still requires user signing; AltStore preservation and actual OS memory allowance require device verification. Entitlement presence does not guarantee extra RAM.
- GitHub CI contains the existing encrypted IPA delivery artifact and integration evidence. No web folder, RPF assets or raw WASM were published. Local authenticated decryption, SHA256, ZIP CRC, source/version/bundle metadata and ARM64 checks passed.
- Evidence archive SHA256 `d5001f17c7a4b2155dc063e2234e21a98c77de1acafb7279ecd7cf65199795f7`; encrypted-delivery archive SHA256 `86e4cd7b067e203472e0dd52739a3b1ebbb835e9d95383e73834dac1a4ca331c`.

## Audit decisions and remaining risks

| Area | Evidence and disposition |
|---|---|
| Guest/Wasmtime | Original shared memory min 512 MiB/max 4 GiB; reserved address space and accessible bytes are not resident pages. No synchronized per-page free-owner contract, so no arbitrary decommit, pool reduction or stack shrink. Existing resident-page scan remains optional, never added to the render hot path. |
| Engine resources | Live texture/buffer objects remain engine-owned. No evidence authorizes discarding them, reducing mips or aliasing render targets. New cache retirement cannot reclaim genuinely live city assets. |
| Pressure handling | Existing debounced policy and one-cold-tail retirement remain. Proactive lifetime retirement fixes a separate ownership defect. If no cold tail exists, current policy may reclaim zero bytes; sustainable headroom is not guaranteed. |
| Readbacks | Opcode 42 already stages and maps asynchronously. The engine can still synchronously require the result. No stale water-height values or fabricated query completion were introduced. New handler/poll timings do not yet measure full consumer latency. |
| Pipelines | Existing warmup and required-pipeline completion remain. No blind cache expansion, hot-pipeline purge or new compilation policy without the missing workload. |
| Audio | Guest PCM ring is bounded; Opus state is keyed by guest address without a verified destroy callback. Blind decoder eviction could corrupt audio and was rejected. No audio callback work added. |
| Common run-loop modes | Inspected code uses them with source/timer registration, which is valid. The reported invalid-mode execution warning needs an iPhone backtrace; changing registration speculatively was rejected. |
| Script metadata | Historical assertions explicitly fall back to ScriptData. No demonstrated relation to termination; original guest metadata remains untouched. |
| Thermal | Supplied session starts serious. No nominal-versus-serious controlled comparison exists. No claim that these fixes reverse throttling. |
| Logging | Only an exact known repetition is aggregated. Unbounded GPU `log_once` key storage remains a potential smaller owner; attributable byte cost is unknown. |

No experiment lowering visual quality or dropping frames was accepted. No measured FPS regression was hidden by comparing unequal scenes. Whole-runtime ASan/TSan, iOS allocator stacks and physical gameplay validation are not implied by component tests.

## Next iPhone verification

1. Preserve the 0.46.5 IPA for rollback. Install the candidate over the same app identity; do not delete the app or selected web/save directories. Confirm version/build/source in Diagnostics and export a fresh session.
2. First use the same mode, save, resolution, quality, population and 30/60 FPS selection as the failing session. Use the same route and camera sequence. Include stationary, fast driving, shadow-heavy and water areas; test audio, controls, save/load and stop/relaunch. Then cover Story/Sandbox, 640×360/1280×720 and 30/60 separately.
3. Run 20–30 minutes when possible. Compare same-route cold/warm runs at similar initial thermal state. Record footprint/headroom, Metal size, guest accessible bytes, live textures/proxies/buffers, retired cache groups/bytes, pressure outcomes, new p50/p95/p99 wall categories, audio and actual visible freezes. Do not add overlapping metrics or call PRESENT rate displayed FPS.
4. Export the complete TXT before stopping and after the next launch. If an abrupt exit occurs, retain the prior-session outcome and fetch the matching iOS Analytics `.ips`/Jetsam report. Correlate process, version/build, UUID and timestamps. A CPU-resource warning with action “none” is not termination proof.
5. For unresolved freezes, collect Metal System Trace / Time Profiler on a device build: actual command-buffer GPU durations, CPU stacks and drawable scheduling. For the run-loop warning, stop on `_CFRunLoopError_RunCalledWithInvalidMode` and capture the caller. These require a physical device/Xcode environment unavailable here.

## Compact report for ChatGPT

Baseline Rebirth0.46.5/build51/source5ce6cf698975a628057dee786297fc65a653fb79, genuine Wasmtime49/wgpu30/Metal. Candidate0.46.6/build52/source e8c7f7e8a66621bfa8e73bdf4b1c6c48d2087153; integration CI40015267583 passed. User-transcribed final footprint3,482,259,960B, headroom57,732,616B, Metal1,681,276,928B, texture estimate1,036,797,532B, engine buffers364,577,512B, liveObj58078. Counters overlap. New raw Israelrebirth log and matching OS report unavailable; termination cause unconfirmed, memory pressure plausible. Old “fps” is PRESENT rate, not actual displayed FPS.

Confirmed fixes: stale numeric-ID bind-group/view/derived-sampler ownership now invalidates selectively on resource destruction/replacement; proxy/counter handles no longer remain cache-owned after retirement. GPU work retains completion-safe ownership. Frame-slot fence moved from end-PRESENT to first subsequent uniform-write/submit, preserving3slots and the same fence while permitting CPU preparation overlap. Removed unused per-submit pending-map Vec. Archive worker drops unused exact/folded maps after validation:20k-entry fixture releases1,560,000 duplicate path bytes+40k map entries. Persistent bounded session journal distinguishes fatal/panic/explicitstop/engineexit/unknown. Exact Cloth warning aggregation preserves counts and first occurrence.

Host streaming fixture:4096resources/128live/fourgroups each, old16384groups/64MiB modeled ownership/23.383ms versus new512groups/2MiB/13.952ms. This is 62MiB modeled retention removed, not iPhone RAM. Metal retirement fixture releases two4MiB proxy cache owners while preserving in-flight pixels.65completehost tests,41portable+5archive tests,40Swift assertions,2entitlement tests passed (Rust counts overlap). Host Metal validates exact pixels/bytes/generations and unchanged3slots. Original-engine host TLS/import/input checks passed without starting gameplay. Separate macOS ASan run40015584798 passed41+5component tests; no whole-runtime sanitizer or iPhone pass claimed.

New fixed21,312-byte telemetry separates PRESENT interval, commandwall, slotwait, submit, acquire, present, poll, readback, pipelinewait, produceridle; categories overlap, not GPU/activeCPU/displaytime. Actual iPhone memory savings, FPS, freeze reduction, 400–600MiB headroom and long-session stability remain UNKNOWN. First device priority: matched route under same settings/thermal state, inspect retirement/reclaim counters plus slotwait/acquire/poll tails, and obtain matching Analytics termination evidence if it exits again.

IPA GTA-Rebirth-0.46.6-e8c7f7e8a666-unsigned.ipa,63,210,014B,SHA256 0bb2f67fd7517ed271d01cebf7557284929953d0674965809ac08cdd60e6b24d; ARM64 UUID66A043C1-30A7-3FD4-AF2A-CD4B65C0AE31. Same app/save identity; archived0.46.5 preserved. Signed metadata requests both memory entitlements, but installed entitlement grant/headroom remains device-dependent.
