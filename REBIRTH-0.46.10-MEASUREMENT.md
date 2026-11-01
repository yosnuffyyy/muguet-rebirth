# Rebirth 0.46.10: Phase 0 measurement

Diagnostic logging only. No rendering, memory or gameplay behaviour is meant
to change. Device results are unverified until a session report is collected.

New lines on every ten-second heartbeat:

- `GPU pass timestamps:` begin/end timestamp for each render pass opened by the
  draw path (not the final blit or helper passes). Reports frame GPU span
  (first pass begin to last pass end, includes idle gaps between submits) and
  the sum of pass durations (passes overlap on Apple GPUs, so it is not busy
  time), plus the slowest pass and its index in the frame. Frames whose
  readback slot is still busy are skipped, never waited on. Disable with
  `MUGUET_GPU_TIMING=0`. The overhead of the timestamps is not measured.
- `uniform packs:` packs and KB staged per PRESENT by `pack_copy`. Repeats of
  identical data are not detected; this sizes the Phase 1 reuse work.
- `host import calls:` top 20 engine-to-host imports by calls in the window,
  from the existing per-import counters (no new per-call work).

How to read them together with the existing wall telemetry:

- GPU span close to the PRESENT interval and high frame-slot waits: GPU bound.
- GPU span well below the PRESENT interval with high command handler wall:
  CPU/driver bound on the render thread.
- Neither: the engine's own frame limiter or simulation threads (not measured
  here) set the pace.

Procedure: same settings as the previous report, load into story mode, stand
still for two minutes, then drive for five minutes, then share the TXT report.
