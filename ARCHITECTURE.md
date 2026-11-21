# Architecture

```
src/
├── lib.rs          plugin: MidiCurvesPlugin, #[persist] state, process_event, GUI wiring,
│                   nih_export_vst3! + nih_export_clap!
├── main.rs         standalone (eframe): App, port panel, settings, repaint watcher
├── midi.rs         standalone only: midir ports and the MIDI callback
├── velocity.rs     decode CC88 + note → x, encode y → 7/14 bit, per-channel prefix,
│                   process_midi (bytes in → bytes out)
├── editor.rs       shared egui editor: CurvesUi (tabs, graph of the selected note, morph lane,
│                   keyboard strip, presets), History<T> (undo/redo of a whole note map)
├── library.rs      preset library: built-in shapes + files; curve presets (format 2) and
│                   "all" presets (format 3, a CurveSet); legacy read
├── settings.rs     standalone settings.json (version 3; migrates 1 and 2)
└── curve/
    ├── shape.rs    CurveShape / Knot — the curve model (from piano_sim), fill_lut
    ├── notemap.rs  NoteMap — per-note curves: key notes, morph lanes, overrides
    ├── bank.rs     CurveBank — lock-free LUTs (one per note) for the MIDI thread
    ├── legacy.rs   exact migration of the old control-point format
    └── mod.rs      CurveSet (both note maps + output format)
tests/
├── realtime.rs     no allocations, writer/reader stress, ns/event benchmark
├── public_api.rs   library → settings → MIDI bytes, per-note maps, in a temp folder
└── fixtures/       legacy/ — old-format presets and settings; v2/ — settings and plugin
                    state of 0.1.0 (one shape per curve)
```

## Curve model

A curve lives in the unit square: x — input velocity 0…1, y — output 0…1.
Knots are sorted by x, the first and last sit at x = 0 and x = 1, and the curve
passes through every knot. Between knots i and i+1 it is a cubic Bézier

  B(t) = (1−t)³·P₀ + 3(1−t)²t·P₁ + 3(1−t)t²·P₂ + t³·P₃,

with P₀ = knot i, P₁ = P₀ + `hout`ᵢ, P₂ = P₃ + `hin`ᵢ₊₁, P₃ = knot i+1. Handle x
offsets are squeezed into their segment (`fit_handle`), so x(t) is monotone and
the curve is always a function of velocity; `eval(x)` finds t by bisection.

A table of the curve (`fill_lut`, x_j = j/1024) is filled in one pass: x grows,
so the segment and t inside it never decrease, and the root of x(t) = x_j is
found by Newton from the previous t (in f64, with a bracket and bisection on flat
spots of x(t)) — 34 µs per table instead of 0.75 ms.

## Per-note curves (`NoteMap`)

Each curve (Note On, Note Off) is a map over the 128 notes:

- **key notes** (`keys`, sorted, A0–C8 = notes 21–108) — curves with the same
  number of knots N;
- **morph lanes** (`lanes`, one per knot i) — handles only; the lane passes
  through the knot's values at the key notes (the key curves are the single
  source of truth);
- **overrides** — single notes with a curve of their own (any number of knots),
  outside the morph.

The curve of note n (`shape_at`): an override if there is one; at a key note —
that key curve as is; below the first / above the last key note — the curve of
that key note; between neighbouring key notes n_A < n < n_B, for every knot i

  x_i(n) = Lˣᵢ(n),  y_i(n) = Lʸᵢ(n),
  hin_i(n) = (1 − t)·hin_i(A) + t·hin_i(B)  (the same for hout),  t = (n − n_A)/(n_B − n_A),

where x_i, y_i are the knot of note n (`Knot::x`, `Knot::y`, unit square),
hin/hout its handles, and Lˣᵢ, Lʸᵢ the lanes (`Lane::x`, `Lane::y`). On
[n_A, n_B] a lane is a cubic Bézier in (u = note/127, v = knot value):
P₀ = (n_A/127, v_A), P₁ = P₀ + `hout`(A), P₂ = P₃ + `hin`(B), P₃ = (n_B/127, v_B)
(`NoteMap::lane_seg`); its u handles are squeezed into the segment, so the lane
is a function of the note, and the value at n is found like `CurveShape::eval`
does. `auto` handles are a third of the chord, hout(A) = (P₃ − P₀)/3,
hin(B) = (P₀ − P₃)/3: the lane is straight and the morph linear. The first and
last knots stay at x = 0 and 1. Bent x lanes of neighbouring knots may cross;
then the knots are clamped in order with the gap `KNOT_GAP` (`clamp_x`), so every
note is still a valid curve (the editor draws such places red).

Operations: `add_key(n)` is exact — the new key curve is the morph at n and every
lane is split at n by de Casteljau, so no note changes (lanes that were straight
stay `auto`). `insert_knot(seg, t)` splits segment `seg` of every key curve at
the same parameter t (the key curves keep their shape); `remove_knot(i)` removes
knot i from all key curves. `fit_shape` brings a curve from the library to N
knots (or adds knots to the map). `normalize` repairs maps from files: sorted
unique key notes in range, equal knot counts, lane lengths, NaN.

## Threads and the MIDI path

| Thread | Plugin | Standalone |
|---|---|---|
| MIDI | host audio thread: `process()` → `process_event` | midir input callback → `velocity::process_midi` → output `send` |
| GUI | baseview: `CurvesUi`, writes LUTs and `#[persist]` state | eframe: `CurvesUi`, writes LUTs, saves settings |
| other | host main thread: state save/load, `initialize()` refills LUTs | repaint watcher: polls the note counter, wakes the GUI; `midi-ports`: every 2 s re-scans ports and reconnects the chosen pair (`Midi::refresh`, shares `Mutex<Midi>` with the GUI only) |

Rules on the MIDI path: atomics only (no `Mutex`/`RwLock`), no allocation,
no syscalls besides the standalone's own `send`, O(1) per event.

- `CurveBank` holds a LUT of `LUT_N` = 1025 points for every (curve, note):
  2 × 128 × 1025 f32 bits in `AtomicU32` (≈ 1 MB, on the heap), `Relaxed`.
  Lookup for note n: f = x·(N−1), j = min(⌊f⌋, N−2), w = f − j,
  y = L_n[j] + (L_n[j+1] − L_n[j])·w (the note number is masked to 0…127). While
  the GUI rewrites a table during a drag a note may read a mix of old and new
  neighbours — an intermediate shape of the same edit, still within [0, 1].
  ≈ 18–20 ns per message across 88 notes (16–17 ns with a single table).
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

- Tables are refilled only for the notes an edit changed: `Outcome::dirty`
  (an edit of key note k — from the previous to the next key note; a knot
  added/removed, undo, a preset — all notes) → `CurveBank::store_map`, which
  computes equal neighbouring curves once. A drag frame with 8 key notes × 6
  knots takes ≈ 1.1 ms (0.84 ms of it refilling tables). The 257-point polyline
  is recomputed only when the shape changes; the faint neighbour curves are read
  from the bank's tables.
- egui-baseview draws a frame only when egui asks for one and does not honour
  `request_repaint_after`, so the plugin requests frames itself: every frame for
  the first second after opening (a frame drawn before the window was mapped was
  lost — the black window in Reaper), then the note marker at ≤ 30 Hz and a
  keep-alive frame at 4 Hz. A closed window costs nothing.
- History: a snapshot of the whole note map is committed when input is
  released; undo/redo per curve, depth 100. In the plugin the history lives in
  the GUI state, not in the project.

## State and files

- Plugin: `#[persist = "curves"]` (`CurveSet` as JSON: `note_on` and `note_off`
  note maps, `hi_res`), `"tab"`, `"editor-state"`. A map is read also in the old
  form `{"knots": [...]}` (0.1.0: one shape per curve) — as one key note (60) with
  that shape; every map read from a file is repaired (`NoteMap::normalize`).
  After a host loads state, `initialize()` refills the LUTs and the GUI adopts the
  new maps (history reset).
- Standalone: `settings.json` version 3 (version 2 = one shape per curve,
  version 1 = control points); before an older file is first overwritten it is
  copied to `settings.v<version>.json`.
- Preset files `<name>.json`: a curve — `{"version": 2, "shape": {...}}` (the same
  `shape` as piano_sim's velocity curves); *all* — `{"version": 3, "set": {...}}`
  (a `CurveSet`). Old files (`control_points`) are read through `curve::legacy`
  and never rewritten.
- Old curves migrate exactly: 2 points — a straight line clamped to [0, 127];
  3+ points — one cubic Bézier P₀…P₃ (first four points) with flat shelves to the
  edges. Checked against the old math on the fixtures (< 0.01 velocity).
