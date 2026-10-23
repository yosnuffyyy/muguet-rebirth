# Project Rebirth — RAM ownership and optimization

## Status and identity

This work targets the genuine `yosnuffyyy/muguet-rebirth` fork, branch `native-ios-prototype`, based on upstream Muguet `1b4600b052b864c78fe36180073ff406663b7e0b`. It does not use the obsolete Stage A or wasm2c runtime.

Baseline executable: `35c28af3f9ca736e241cd97dbcc596360a60e079`, Rebirth **0.46.4 / build 50**. Its build passed integration run **39700098729**. The initial RAM changes are commit `83f6b198f4db222f32276498b3bfcbb4d914b27f`; the validated executable is **0.46.5 / build 51**, `5ce6cf698975a628057dee786297fc65a653fb79`. [Integrated CI run 40007555943 passed](https://github.com/yosnuffyyy/muguet-rebirth/actions/runs/40007555943). Host/Metal validation and packaging are complete; physical-device memory, FPS and gameplay verification remain pending.

The user confirms the installed version is unknown and the black-screen log is the latest available gameplay log. Consequently, there is **no fresh matched physical-device baseline** and no measured percentage reduction in iPhone footprint. The 25–35% goal and 40% stretch target are not demonstrated. Neither sustained FPS nor leak-free long sessions can be established from host tests.

Original engine SHA256: `e65bec476e8fd8125515150c24037c9bd0d0c46530e3544f9d389ab108b5592e`; shader build `8b0b5899ed`. Its original bytes, ABI, gameplay pools and timing are unchanged. iOS still executes Wasmtime 49 AOT code and wgpu 30 / Metal. Texture quality, dimensions, mip selection, effects, population, audio, physics, save namespaces and the 30/60 FPS selection are unchanged by this pass.

## Evidence and accounting

Historical file: `Blackrebirth-1792626002794032000-0.txt`, 9,277,363 bytes, SHA256 `74078e66df4d6e01f437eaaf2a146abf7365cf4691d4dd456af2de21cd51f4d3`.

The last detailed session (`1792625843180833000`) used **Sandbox, 1280×720 Custom, texture 1, FXAA enabled, shadows 1, reflections 2, postFX 2, anisotropy 8, population 1.0**. It is not a 640×360 low-quality comparison. It predates the resolve/presentation repair and contains 6,340 validation errors and 3,465 skipped draws. Its memory evidence is useful; its frame cadence cannot establish performance of the corrected renderer.

| Historical sample | Value | Scope / limitation |
|---|---:|---|
| Final sampled process footprint | 3,020,624,184 bytes | OS footprint accounting, not sum of the rows below |
| Available memory at that sample | 519,368,392 bytes | Advisory OS headroom; changes with system pressure |
| Guest accessible memory | 1,803,616,256 bytes | Wasmtime accessible linear range, **not resident guest pages** |
| Prior Metal allocation sample | 1,499 MiB | `MTLDevice.currentAllocatedSize`, not measured GPU residency |
| Prior compressed-memory sample | 776 MiB | Overlaps process accounting; sampled earlier |
| Last guest malloc report | 1,590 MiB arena / 1,539 MiB in use | Guest allocator accounting, not native malloc or OS footprint |
| Guest game virtual pool | 343 / 352 MiB | Nested guest pool accounting |
| Guest resource virtual pool | 551 / 576 MiB | Nested guest pool accounting |

**Do not add these counters.** They overlap and some were sampled at different times. Approximately 51 MiB of guest arena slack is not proof of safely reclaimable resident pages. No matching jetsam/crash report was available; memory termination is unconfirmed.

Existing renderer `fps` logs count PRESENT requests, not actual drawable completions. The older `pipelines` label counts demand creations, excluding prewarmed pipelines: 325 warmed recipes plus 181 demand creations do not mean 181 live pipelines. Real cache allocation bytes, actual displayed frame percentiles, GPU residency and resident guest pages remain unknown for the current build.

## Implemented ownership changes

### 1. One CPU uniform stage, three unchanged GPU frame regions

Files: `src/gpu.rs`, `src/gpu_memory.rs` and the associated tests.

The previous implementation retained one CPU staging vector for each of three GPU frame slots. GPU completion protection is necessary for the GPU regions, but not for CPU bytes after `wgpu::Queue::write_buffer` copies them into owned queue staging. The pinned backend's synchronous data-copy contract was checked before removing the duplicate CPU ownership.

Each uniform chunk now has one reusable CPU stage. The GPU buffer still contains three 8 MiB regions; submission order, slot completion fences, offsets and alignment are unchanged. Dirty intervals are copied before submission; packed draw constants and padding are preserved.

For a fully populated chunk, retained CPU capacity is **24 MiB → 8 MiB**, a **16 MiB reduction**. GPU capacity remains 24 MiB. Smaller chunks save according to their actual vector capacities. This is an attributable owner-level reduction, not a claim that OS footprint falls by precisely that amount immediately.

The production Metal fixture passed actual upload, uniform packing, binding, PRESENT, all three frame regions, retirement and replacement, including fourteen correct draw pixels and zero validation errors.

### 2. Borrow immutable command payloads until wgpu copies them

Files: `src/gpu.rs`, `src/gpu_memory.rs`.

Buffer and ordinary/native-BC texture uploads previously copied the inline command payload into a new native vector before passing it to wgpu. The producer cannot reuse the command-ring span until the consumer advances the tail after execution. That ownership interval covers wgpu's synchronous copy, so the extra snapshot is removed. Conversion paths retain their required owned output; ASTC conversion is not rewritten.

Payload ranges are checked for overflow and guest bounds. No pointer survives command execution. Counters record avoided cumulative snapshot bytes and the maximum individual payload; **cumulative traffic is not retained RAM**. A regression overwrites guest input after upload returns and verifies the GPU still receives the original pixels.

### 3. Bounded synchronous file transfers

Files: `src/fs.rs`.

Synchronous read/write/pread/pwrite formerly allocated an entire request-sized native payload. They now use one **64 KiB** scratch buffer under the existing filesystem lock. All iovec descriptors are snapshotted before reading because valid destinations can overlap the descriptor table. Guest bounds, returned-count pointers, offset overflow, EINTR, short reads and partial writes remain checked. Positioned operations preserve the file cursor; stdout/stderr continue through the logging path.

The comparable large-file fixture uses **37,314,560 bytes**: request payload capacity changes from 37,314,560 to 65,536 bytes, avoiding **37,249,024 bytes** during that operation. The new 64 KiB scratch remains allocated while its filesystem owner is live; this trades small bounded idle retention for eliminating request-sized peaks. The fixture size comes from the earlier transition failure; it does not prove that this recent Muguet session made the same synchronous request. The asynchronous archive reader already had a 64 KiB buffer and is not credited with a new saving.

### 4. One immutable manifest owner

Files: `src/rebirth/assets.rs`, `layout.rs`, `mod.rs`, `src/io.rs`.

Launcher validation, the reader and runtime publication now share immutable frozen manifest bytes through `Arc<[u8]>`. Two whole raw-manifest copies are removed; the parsed lookup index remains because it serves a different purpose. Savings are twice the actual manifest length, at most 16 MiB under the existing 8 MiB bound. This neither rescans nor relocates game files.

### 5. Stream pipeline warmup recipes

Files: `src/pipeline_recipes.rs`, `src/gpu.rs`.

Loading no longer keeps the complete JSONL text plus an expanded queue of every parsed recipe. A 16 KiB reader and one reusable line buffer supply one recipe at a time. Valid recipe order, warmup deadline, eight-job limit, compilation semantics and normal required-draw compilation are preserved. The file length is frozen at open so newly appended recipes do not extend their own warmup.

Malformed lines retain the previous filtering behavior. I/O failures and a record exceeding the existing 32 MiB cache budget report a specific error; required draws still compile normally. A small fixture compares eager/streamed values and order; a separate synthetic near-32 MiB cache verifies recipe count and bounded reads without retaining the complete input or parsed queue. The reusable line can retain its largest record, up to the 32 MiB guard, until warmup finishes; one parsed value has additional allocation cost. This is not a universal 16 KiB total-memory bound. The historical 325-recipe file size is unknown, so its actual byte saving is unknown.

### 6. Reclaim only a demonstrably cold uniform tail

Files: `src/gpu.rs`, `src/memory_budget.rs`, `src/startup.rs`, `ios/Launcher.swift`.

The iOS memory-pressure source publishes atomic warning/critical state. An existing background diagnostics worker samples OS headroom and footprint every five seconds. Full ordinary logs remain every five seconds initially and every thirty seconds after startup. A coalesced request is generated for an OS warning, critical headroom, sustained low headroom, or sustained growth while headroom is low. Requests are debounced for sixty seconds, with one immediate severity escalation.

At a PRESENT boundary, the GPU owner may retire **at most one trailing uniform chunk unused for 120 PRESENTs**. It always retains the first chunk, refuses dirty storage, removes cached binding references to the retired unique ID, and gives replacements a new ID. GPU handles are dropped rather than forcibly destroyed; recorded/in-flight wgpu work retains ownership. No global GPU wait is added and hot pipelines are not purged.

One eligible full tail releases host ownership of up to 8 MiB CPU capacity and 24 MiB logical GPU capacity. Actual driver release time and footprint change require device observation. If there is no eligible tail, the request cannot reclaim memory. If rendering is suspended, it waits for a safe PRESENT boundary. This is a conservative cache-reclamation manager, not a complete engine streaming-budget controller.

Headroom thresholds are advisory, not hard allocation limits: below 128 MiB requests immediately; below 256 MiB requires two samples; growth of at least 128 MiB over at least thirty seconds only triggers with headroom below 512 MiB. Entitlement presence never substitutes for the OS-reported budget. These thresholds do not change engine pools, graphics quality or permitted guest size.

### 7. Respect the imported memory maximum

Files: `src/host.rs`, `src/rebirth/heap_growth.rs`.

The old growth policy used a fixed 16 GiB ceiling even though the original imported memory's maximum is 4 GiB. Growth candidates now respect the declared maximum, preserve the existing normal 20%/10%/5% over-allocation policy and 96 MiB excess cap, and attempt exact required growth as a final fallback if speculative growth fails. Duplicate candidates are not retried.

This is a correctness/availability repair near the limit, not a measured normal-session footprint reduction. Linear memory is not shrunk or unsafely decommitted.

### 8. Fill nonzero UAV clear staging directly

Files: `src/gpu.rs`, `src/gpu_memory.rs` and memory tests.

The existing full-buffer nonzero UAV clear built a destination-sized native `Vec`, then copied it into another destination-sized wgpu staging allocation. It now fills the supported [`Queue::write_buffer_with`](https://docs.rs/wgpu/30.0.1/wgpu/struct.Queue.html#method.write_buffer_with) view directly. Dropping that view schedules the same copy before the next submission. The preceding flush, zero-clear path, four-word pattern, destination and command ordering are unchanged. A failed staging allocation remains an explicit failure.

A 64 MiB synthetic clear eliminates 64 MiB of extra native payload capacity and one full copy; the required 64 MiB queue staging and destination buffer remain. Actual device clear sizes/frequency are unknown and now counted. Production Metal tests cover 4-, 20- and 4,096-byte full-buffer views, non-16-byte tails, two patterns, zero clears, before/between/after copies, readback offsets and canaries. This optimization preserves the existing full-buffer semantics; it does not claim to repair previously unverified typed or partial-view UAV semantics.

## New measurements and overhead limits

The launcher now displays the installed **Rebirth version and build number** on every page. Diagnostics and exported logs include the exact source revision injected into the signed bundle during packaging. Each new session writes a durable identity checkpoint. Export metadata identifies the exporting app explicitly; it does not relabel retained sessions from older versions. Missing identity values display `Unknown` rather than an assumed build.

The existing diagnostics worker records exact-byte footprint, resident size, compressed memory, current available memory, Metal allocated size, sampled footprint peak and thermal state. Unknown values are explicit. Sampling does not walk guest pages or run on the audio callback.

A renderer ownership census runs at the existing ten-second statistics boundary: engine object count, native buffer capacity, constant mirrors, texture/proxy texel estimates, uniform GPU capacity, CPU staging capacity, ASTC scratch, avoided upload/clear copies and retired uniform capacity. It also records actual pipeline, pending-job, bind-group and layout entry counts, warm/map program and variant entries, zero-fill/copy scratch sizes and census scan wall microseconds. Program/variant entries are not a deduplicated native allocation size. It walks owned resources once per interval, not per draw. Texture estimates exclude driver padding, metadata and residency. Cross-thread heartbeat fields are independently sampled and may span adjacent censuses; the GPU-owner census line itself is produced on the GPU thread. These measurements must not be added to process or Metal totals.

Pressure callbacks perform only atomic stores/counting. Escalation is handled on the next five-second sample, then the next safe PRESENT, rather than synchronously in the OS notification. There is no new busy loop, per-frame OS memory query, signal-handler allocation or synchronous pressure purge. Source inspection establishes these bounds; iPhone sampling cost and sustained performance still need measurement. Census duration excludes formatting/log I/O; sampled peak is explicitly unknown until a footprint query succeeds.

Apple documents available memory as a changing advisory value, not an entitlement-guaranteed allocation: [os_proc_available_memory](https://developer.apple.com/documentation/os/os_proc_available_memory). The notification mechanism uses the supported [Dispatch memory-pressure source](https://developer.apple.com/documentation/dispatch/dispatchsource/makememorypressuresource(eventmask:queue:)).

## Full-runtime audit and decisions

| Owner / subsystem | Finding and disposition |
|---|---|
| Original guest engine | 66,172,903-byte WASM; 91,026 defined functions, one fixed 89,368-entry function table, 57 globals and 4,077 passive data segments totaling 7,818,183 bytes. Original malloc/buddy functions identified, but no synchronized host-visible free-page ownership contract. No arbitrary address patch, pool shrink or decommit. |
| Guest linear memory | Logical accessible growth is not physical commitment. Existing 20/10/5% policy is retained within the declared maximum. Resident pages and allocator call stacks require device profiling. |
| Worker instances | Module and SharedMemory are shared. Per-instance table/runtime metadata exists but its byte cost is unmeasured. Do not assume all AOT code/passive data are copied per worker. |
| Native stacks | Worker/main/GPU reservations are 16/64/32 MiB. Reserved address space is not touched resident memory; shrinking without worst-case stack evidence risks traps. |
| Wasmtime/Cranelift | iOS loads embedded signed AOT code; no runtime Cranelift compilation. Prior `__GAME` segment was 158,433,280 bytes, file-backed RX. No speculative compiler/allocator change. |
| Engine textures | Native BC supported by the historical A18 Pro adapter. No evidence that BC decompression dominates that run. No quality, format, mip or resolution reduction. |
| Constant-buffer mirrors | Pure constant buffers deliberately have no dedicated GPU buffer; the mirror is authoritative until packing. Mixed-use buffers require both semantics. Removing all mirrors would corrupt constants. |
| Views/proxies/counters | Some cache references can retain proxy/counter handles after source destruction. Byte totals and lifetime-specific reclamation need evidence; no forced destruction of queued resources. |
| Shader/program/pipeline caches | Session-retained recipes, modules, variants and layouts need generation-aware ownership. Blind eviction risks the earlier compilation thrash. Streaming recipe input is fixed; compiled hot objects are not purged. |
| Asset reader | Existing bounded 64 KiB scratch and eight file descriptors preserved. No whole-archive host cache introduced. |
| Audio output | Existing 16,384-frame stereo f32 ring is about 128 KiB plus header and already read directly. No giant duplicate native PCM queue found. |
| Opus | Decoder map lifetime lacks a proven guest destructor hook. Blind LRU eviction would lose decoder history. Reusable transient PCM scratch would add idle retention without measured RAM benefit, so deferred. Existing shared-memory aliasing deserves a separate focused safety review. |
| UI and logs | Launcher dismissed before play. Folder security scope intentionally remains live. Eight × 32 MiB logs are disk retention, not 256 MiB heap; export uses 32 KiB scratch. No new launcher animation/worker. |
| Session lifecycle | Runtime and audio/worker globals live for one process session; stop exits that process. Repeated-launch tests must distinguish process restart from in-process reuse. |
| iOS driver/compression | Metal allocation counters and compression overlap other accounting. Releasing a Rust handle is not proof of immediate driver deallocation. No forced collection or unsafe aliasing. |

The largest known memory scopes remain guest allocations and aggregate Metal allocations. The texture share versus buffers, pipelines and driver retention awaits the new census. A 25–35% reduction cannot be justified by summing virtual reservations or by counting these safe owner reductions multiple times.

## Ranked remaining memory work

The table ranks remaining investigation by known owner scale and safety, not by an invented savings percentage. The implemented duplicate-storage reductions above are independently testable; the larger owners below need a current device trace before changing their lifetimes.

| Priority | Owner and evidence | Next justified action | Savings / risk / performance constraint |
|---|---|---|---|
| 1 | Guest allocator: historical 1,539 MiB in use, 1,590 MiB arena | Capture allocation owners across the same scene transition and replay route; distinguish live world/script/physics data from reusable temporary pages | Potential savings UNKNOWN. High risk without synchronized free-page ownership. Do not shrink live pools or modify arbitrary WASM addresses. |
| 2 | Metal allocations: historical 1,499 MiB, scope includes more than textures | Use the new texture/buffer/proxy census with Metal allocation tracing to locate duplicate or obsolete owners | Savings UNKNOWN. Preserve mip chains, quality, queued command references and streaming reuse; no purge based solely on age. |
| 3 | Proxy/counter views retained by caches after source destruction | Add a destruction/reuse fixture that follows cached bind groups and resource generations, then release only obsolete cache ownership | Byte cost UNKNOWN. Correctness and recompilation/cache-hit impact must be measured before broad invalidation. |
| 4 | Shader/program/layout caches retained for a session | Measure unique live identities and backing allocations versus repeated use before designing selective eviction | Savings UNKNOWN. Earlier builds proved pipeline thrashing can severely hurt responsiveness. No blanket pressure shrink. |
| 5 | Per-worker Wasmtime instance tables/runtime metadata, roughly 44–45 historical workers | Profile native allocations by instance; identify upstream-supported sharing while retaining TLS/table semantics | Savings UNKNOWN. Stack reservations and shared AOT code are not attributable resident duplicates. |
| 6 | Opus decoder history and streaming scratch | Establish the guest destruction contract and active voice lifetime before reclaiming decoder state | Savings UNKNOWN. Decoder history must survive packet boundaries; no forced eviction or audio-quality reduction. |

No verified host-side ownership evidence supports freeing hundreds of MiB of required guest or texture data yet. This is the blocker to a defensible 25–35% footprint claim, rather than a reason to change quality or report virtual-space reductions as RAM savings.

## Before/after validation

Final integrated validation passed on macOS 15, Rust 1.98.1, the host Metal adapter, and iPhone 16 Pro Simulator / iOS 26.2 for launcher previews. The first RAM revision also passed. An intermediate revision failed compilation because wgpu 30 requires `WriteOnly` access rather than a mutable slice, and a test pattern needed an explicit u32 type. Both were corrected in the final executable revision; the failed revision was not delivered. No infrastructure retry or bypass was needed for the final run.

| Check | Result / limits |
|---|---|
| Entitlement validation | 2 Python tests passed; actual `codesign --verify --strict` and signed-entitlement extraction passed on macOS |
| Swift settings and identity | 40 assertions passed, including saved preferences, modes, presets, invalid/missing identity and source revision |
| Portable Rust adapters / assets | 27 + 5 tests passed; some overlap the host suite, so these are not claimed as 32 additional unique behaviors |
| Complete host binary suite | 43 passed, 0 failed/ignored; includes file/save/ABI/input/pressure/growth/recipe regressions and production Metal tests |
| Production upload/lifetime pixels | 14 correct pixels across three slots; cold-tail retirement and new binding identity; borrowed texture payload survives guest overwrite; UAV 4/20/4096-byte patterns, tails, zero clears, ordering and canaries; zero validation errors |
| Existing graphics regression | Metal resolve/copy, MRT and final-blit pixel tests passed |
| Original engine integration | Verified original module hash, TLS/constructors, actual Wasmtime imports and input dispatch: two modes × two resolutions × repeated publication, held/released key and invalid pointers. World/main/GPU gameplay was not started by this fixture |
| Launcher previews | Seven actual Swift pages captured; Home and Diagnostics inspected for visible version/source. Preview app uses its separate simulator bundle ID and is not the game runtime |
| iOS build | Compiler-free ARM64 Rust/AOT build passed; no wasm2c, wgpu-native or runtime Cranelift compiler dependencies |
| IPA delivery | Authenticated decryption, archive digest/CRC, ARM64 Mach-O, signed metadata, version/build/source, unchanged production bundle ID and no web/game archives verified |
| Sanitizers | ASan/UBSan/TSan were not executed by the available workflow. Local Windows executable tests were blocked by GNU CRT/toolchain linking; macOS CI executed the regression suite. Bounds/canary/Metal validation tests are not a sanitizer substitute |
| Physical iPhone | No new run available: footprint reduction, actual display FPS, audio playback, thermal impact, 20–30-minute plateau and gameplay remain unverified |

| Comparable workload | Before | After | Provenance |
|---|---:|---:|---|
| Fully populated uniform chunk CPU capacity | 24 MiB | 8 MiB | Exact owner capacity; host benchmark and production pixel fixture passed |
| Same chunk GPU capacity | 24 MiB | 24 MiB | Unchanged three-slot allocation |
| 37,314,560-byte synchronous transfer payload | 37,314,560 B | 65,536 B | Controlled real-file fixture passed, checksum 4,757,606,400; positioned cursor retained |
| Immutable raw manifest owners | 3 | 1 | Shared immutable storage; parsed index remains |
| Pipeline warmup input | Whole file + all parsed recipes | 16 KiB reader + one line + one recipe | Small equivalence/order test passed; separate 33,554,432-byte / 32,768-recipe fixture used a 1,024-byte peak line. Parsed-value allocation excluded |
| 64 MiB nonzero UAV clear, extra host payload | 64 MiB | 0 | Required 64 MiB queue staging remains; host and production Metal fixtures passed |
| Current iPhone process footprint / FPS | UNKNOWN | UNKNOWN | Fresh matched device runs required |

Single-run **host wall-time observations**, not iPhone CPU/GPU/frame time:

| Fixture | Before | After |
|---|---:|---:|
| 90 synthetic uniform frames × 8 MiB, including queue-copy stand-in | 61.352083 ms | 40.713541 ms |
| 64 MiB UAV pattern fill / required staging copy | 32.823417 ms | 5.919167 ms |
| 37,314,560-byte file read and guest copy | 17.018500 ms | 6.630625 ms |

These run sequentially before/after inside a concurrently executing unit-test suite, without randomized order or confidence intervals. Filesystem caches, page faults and allocator warming affect them. They demonstrate no gross slowdown in these fixtures, not a defensible sustained speedup prediction. Exact buffer capacities and byte/pixel correctness are the stronger evidence. No fresh physical process-footprint, FPS, streaming-delay or audio-underrun comparison is available. Required queue upload traffic is unchanged where only a duplicate host copy was removed.

## Release artifact

- Executable source: `5ce6cf698975a628057dee786297fc65a653fb79`; version **0.46.5**, build **51**.
- IPA: `GTA-Rebirth-0.46.5-5ce6cf698975-unsigned.ipa`, **63,176,040 bytes**.
- SHA256: `da13ab03cb276bce874da3e13c260c5699a69b81737b4ed7a252da3ea040760f`.
- Mach-O ARM64 UUID: `2567C054-665B-389A-982E-7621BF329C58`.
- Bundle remains `io.github.yosnuffyyy.gtanativeprobe`; display name remains **GTA V (Test Build)**. Both increased-memory-limit and extended-virtual-addressing entitlements are present in the ad-hoc signature; user installation signing and the OS determine the actual grant.
- Local IPA: `outputs/GTA-Native-Prototype/GTA-Rebirth-0.46.5-5ce6cf698975-unsigned.ipa` relative to the native workspace root. Requires sideload signing.
- [GitHub CI](https://github.com/yosnuffyyy/muguet-rebirth/actions/runs/40007555943) contains the existing encrypted delivery artifact and integration evidence. No `web` files, RPF archives or raw original WASM were added to the public artifact. The IPA contains the established signed AOT executable.
- Previous 0.46.4 IPA retained unchanged, SHA256 `323ce4a3b731712892ded3b02cd405036c10bf43ae63a8b408bac8d7b12811c9`.

The report/evidence publication commit may follow the executable commit; installed identity and this IPA always refer to the executable revision above.

## Physical-device collection procedure

1. Record launcher version/build/source and export the log before changing settings. Keep the prior IPA for rollback; install using the same bundle identifier.
2. Compare at the **same** mode, save, location, resolution, texture/effect settings, FPS limit and optional display-probe state. Begin with 640×360; separately repeat the historical 1280×720 profile if desired. Do not compare different quality presets as memory optimization.
3. Start from a fresh process. Sample launcher, startup, two minutes stationary, a repeatable driving route, interiors/exteriors and heavy streaming; continue 20–30 minutes. Record elapsed wall time and location markers with the exported log.
4. Verify real image quality, character/vehicle collisions, audio, controller/touch switching, pause/resume, background/foreground, stop and relaunch. Test Story and Sandbox independently and confirm their existing saves remain intact.
5. Compare footprint/headroom and ownership census at matching stages, peaks and steady state. Check cold-tail retirement, later reallocation, cache reuse, validation-error count, streaming failures, audio underruns and frame pacing. Avoid summing guest/Metal/process counters.
6. For call stacks/residency, capture Instruments Allocations/VM Tracker and Metal System Trace on an attached physical device. Guest live allocation ownership requires a validated engine allocator trace, not an unsynchronized memory walk. A genuine displayed-frame trace is required for FPS p50/p95/p99.
7. If the OS kills the app, retain the matching `.ips`/JetsamEvent report with executable version/UUID. A log ending near low headroom alone does not prove jetsam.

## Compact report for independent analysis

Project Rebirth remains original-engine Wasmtime49 AOT plus wgpu30/Metal. Baseline executable is 0.46.4 build50, `35c28af3f9ca736e241cd97dbcc596360a60e079`. The installed phone version is unknown; the user confirms the latest available log predates the black-screen fix. Historical final footprint was 3,020,624,184 B, headroom 519,368,392 B, guest accessible range 1,803,616,256 B. Earlier Metal sample was 1,499 MiB and compressed sample 776 MiB. These scopes overlap and cannot be added. The run used 1280×720 with higher effects and contained rendering validation failures.

Implemented RAM changes: one CPU uniform stage shared across three unchanged GPU regions (24→8 MiB CPU per full chunk); borrow command-ring upload bytes until wgpu's synchronous copy; 64 KiB synchronous file transfer scratch replacing request-sized payloads; one shared immutable raw manifest; streaming pipeline JSONL warmup instead of a whole parsed queue; bounded pressure-driven retirement of one cold trailing uniform chunk with generation-aware cache invalidation; imported-memory-limit-aware growth with exact fallback; and direct write-only UAV clear staging, avoiding an extra destination-sized host vector (64 MiB in the controlled fixture). Installed version/build/source is now shown in the launcher/Diagnostics and session logs. No graphics-quality, simulation, audio, original engine ABI or save changes. No measured physical-device footprint/FPS improvement is claimed.

Final executable: **0.46.5 build 51**, source **5ce6cf698975a628057dee786297fc65a653fb79**; CI **40007555943 passed**. Validation: 40 Swift assertions, 27 portable + 5 asset tests, 43 complete host tests (counts overlap), original-engine TLS/import/input integration, ARM64 AOT packaging. Metal production tests verified fourteen draw pixels across three slots, cold retirement/reallocation, borrowed-upload contents and UAV clear patterns/order, with zero validation errors; previous resolve/MRT/final-blit regressions passed. ASan/UBSan/TSan and physical iPhone gameplay were not run.

Host single-pass timings: uniform staging 90 × 8 MiB frames **61.352083→40.713541 ms**, retained CPU **25,165,824→8,388,608 B**; 64 MiB UAV clear **32.823417→5.919167 ms**, extra host snapshot **67,108,864→0 B** while required driver staging remains; 37,314,560-byte file read/copy **17.018500→6.630625 ms**, payload capacity **37,314,560→65,536 B**. Timing comparisons are order/cache-sensitive unit-test observations, not sustained FPS predictions. Pipeline fixture: 33,554,432 input bytes / 32,768 recipes, streamed one parsed value at a time with 16 KiB reader and 1,024-byte peak line in that workload. Actual phone savings depend on live owner sizes and workloads; they cannot be summed into a percentage.

IPA **GTA-Rebirth-0.46.5-5ce6cf698975-unsigned.ipa**, **63,176,040 B**, SHA256 **da13ab03cb276bce874da3e13c260c5699a69b81737b4ed7a252da3ea040760f**; ARM64 UUID **2567C054-665B-389A-982E-7621BF329C58**. Same bundle/save identity, prior IPA preserved. Next test: record displayed version, repeat the same Story/Sandbox scene/settings for 20–30 minutes, export the new ownership/pressure log, verify picture/audio/controls/streaming, and compare actual displayed-frame measurements plus footprint/headroom at matching phases. Neither the 25–35% RAM target nor unchanged sustained FPS is physically established.

The next most important evidence is a current, same-settings device memory census and allocation trace. Guest allocations and aggregate Metal allocations dominate known scopes; texture share remains unknown. Arbitrary eviction/decommit would endanger correctness. Remaining unknowns include actual guest resident pages, native cache bytes, queued driver retention, Opus decoder ownership, long-session memory plateau and displayed FPS. The numerical footprint targets remain goals, not results.
