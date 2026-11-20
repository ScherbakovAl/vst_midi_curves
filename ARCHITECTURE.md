# Architecture

```
src/
├── lib.rs          plugin: MidiCurvesPlugin, #[persist] state, process_event, GUI wiring,
│                   nih_export_vst3! + nih_export_clap!
├── main.rs         standalone (eframe): App, port panel, settings, repaint watcher
├── midi.rs         standalone only: midir ports and the MIDI callback
├── velocity.rs     decode CC88 + note → x, encode y → 7/14 bit, per-channel prefix,
│                   process_midi (bytes in → bytes out)
├── editor.rs       shared egui editor: CurvesUi (tabs, graph, presets), History (undo/redo)
├── library.rs      preset library: built-in shapes + files, format version 2, legacy read
├── settings.rs     standalone settings.json (version 2, legacy migration)
└── curve/
    ├── shape.rs    CurveShape / Knot — the curve model (from piano_sim)
    ├── bank.rs     CurveBank — lock-free LUTs for the MIDI thread
    ├── legacy.rs   exact migration of the old control-point format
    └── mod.rs      CurveSet (both curves + output format)
tests/
├── realtime.rs     no allocations, writer/reader stress, ns/event benchmark
├── public_api.rs   library → settings → MIDI bytes, in a temp folder
└── fixtures/legacy old-format presets and settings (migration checks)
```

## Curve model

A curve lives in the unit square: x — input velocity 0…1, y — output 0…1.
Knots are sorted by x, the first and last sit at x = 0 and x = 1, and the curve
passes through every knot. Between knots i and i+1 it is a cubic Bézier

  B(t) = (1−t)³·P₀ + 3(1−t)²t·P₁ + 3(1−t)t²·P₂ + t³·P₃,

with P₀ = knot i, P₁ = P₀ + `hout`ᵢ, P₂ = P₃ + `hin`ᵢ₊₁, P₃ = knot i+1. Handle x
offsets are squeezed into their segment (`fit_handle`), so x(t) is monotone and
the curve is always a function of velocity; `eval(x)` finds t by bisection.

## Threads and the MIDI path

| Thread | Plugin | Standalone |
|---|---|---|
| MIDI | host audio thread: `process()` → `process_event` | midir input callback → `velocity::process_midi` → output `send` |
| GUI | baseview: `CurvesUi`, writes LUTs and `#[persist]` state | eframe: `CurvesUi`, writes LUTs, saves settings |
| other | host main thread: state save/load, `initialize()` refills LUTs | repaint watcher: polls the note counter, wakes the GUI |

Rules on the MIDI path: atomics only (no `Mutex`/`RwLock`), no allocation,
no syscalls besides the standalone's own `send`, O(1) per event.

- `CurveBank` holds two LUTs of `LUT_N` = 1025 points (f32 bits in `AtomicU32`,
  `Relaxed`). Lookup: f = x·(N−1), j = min(⌊f⌋, N−2), w = f − j,
  y = L[j] + (L[j+1] − L[j])·w. While the GUI rewrites a table during a drag a
  note may read a mix of old and new neighbours — an intermediate shape of the
  same edit, still within [0, 1].
- The CC88 prefix (`velocity::Prefix`, one slot per channel) is owned by the MIDI
  thread: set by CC88, consumed by the next Note On/Off of that channel, no timers
  (offline render = realtime).
- Output keeps the input timing: in hi-res the new CC88 goes right before the note
  in the same sample. Plugin latency is 0. The standalone sends from the same
  callback (no queues); its output connection belongs to the callback closure,
  so changing a port recreates the connection.
- Feedback to the GUI goes only through atomics (last input x per curve, note
  counter). The MIDI thread never calls `request_repaint()`.

## GUI and CPU

- The LUT (1025 evaluations) and the 257-point polyline are recomputed only when a
  shape changes.
- egui-baseview draws a frame only when egui asks for one and does not honour
  `request_repaint_after`, so the plugin requests frames itself: every frame for
  the first second after opening (a frame drawn before the window was mapped was
  lost — the black window in Reaper), then the note marker at ≤ 30 Hz and a
  keep-alive frame at 4 Hz. A closed window costs nothing.
- History: a snapshot of a shape is committed when input is released; undo/redo
  per curve, depth 100. In the plugin the history lives in the GUI state, not in
  the project.

## State and files

- Plugin: `#[persist = "curves"]` (`CurveSet` as JSON), `"tab"`, `"editor-state"`.
  After a host loads state, `initialize()` refills the LUTs and the GUI adopts the
  new shapes (history reset).
- Preset files: `<name>.json` = `{"version": 2, "shape": {...}}` — the same `shape`
  as piano_sim's velocity curves. Old files (`control_points`) are read through
  `curve::legacy` and never rewritten.
- Old curves migrate exactly: 2 points — a straight line clamped to [0, 127];
  3+ points — one cubic Bézier P₀…P₃ (first four points) with flat shelves to the
  edges. Checked against the old math on the fixtures (< 0.01 velocity).
