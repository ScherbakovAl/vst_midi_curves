# MIDI Curves

[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.24408082.svg)](https://doi.org/10.5281/zenodo.24408082)
[![CI](https://github.com/ScherbakovAl/vst_midi_curves/actions/workflows/ci.yml/badge.svg)](https://github.com/ScherbakovAl/vst_midi_curves/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/ScherbakovAl/vst_midi_curves)](https://github.com/ScherbakovAl/vst_midi_curves/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Formats: VST3 | CLAP](https://img.shields.io/badge/formats-VST3%20%7C%20CLAP-blue.svg)](#vst3-or-clap)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey.svg)](#download)
[![Rust](https://img.shields.io/badge/Rust-stable-orange.svg?logo=rust)](https://www.rust-lang.org/)

MIDI velocity curves as a **VST3/CLAP plugin** and a **standalone app** (Rust,
[nih-plug](https://github.com/robbert-vdh/nih-plug) + egui). Every Note On and
Note Off goes through its own editable Bézier curve, like *Curves* in an image
editor. High-resolution (14-bit) velocity via the CC88 prefix is supported on
input and output.

![MIDI Curves standalone: Note On curve with the last note marker, MIDI ports and presets](img/standalone.png)

*Standalone app. The plugin shows the same editor without the MIDI panel.*

Video demo (older version): [YouTube](https://www.youtube.com/watch?v=CtjtueLHJKM)

## Features

- Two curves: **Note On** and **Note Off** (release velocity), each with its
  own undo/redo history.
- The curve passes through its knots; each knot has Bézier handles. The curve is
  always a function of velocity (handles are kept inside their segment).
- **Hi-res velocity (CC88, MIDI CA-031).** Incoming CC88 is always decoded:
  input x = (128·HH + LL)/16383 when a CC88 preceded the note on that channel,
  otherwise x = HH/127. The *Hi-res output* switch selects the output format:
  14 bit (CC88 + note, in the same sample) or plain 7 bit.
- A Note On never leaves with velocity 0 (that would be a Note Off).
- Everything else (CC, pitch bend, aftertouch, SysEx, clock) passes unchanged.
- Shared preset folder for the plugin and the app:
  `~/.config/vst_midi_curves/presets` (macOS: `~/Library/Application Support/…`,
  Windows: `%APPDATA%\…`). Italic entries are built in. A click loads a preset
  into the open tab; *Save* / *Overwrite* writes the current curve under the typed
  name, *Delete* removes the file (click twice), *Rescan* re-reads the folder.
- Plugin state (both curves, output format, open tab) is saved in the DAW project.
- Realtime path: no locks, no allocations, O(1) per note (~17 ns per message);
  standalone loopback through ALSA adds ≈ 0 µs on top of midir (p99 69 µs total).

## Editor

| Action | Gesture |
|---|---|
| add a knot (shape unchanged) | click on the curve |
| move a knot / handle | drag (Shift — fine ×0.1, Ctrl — snap to 1/16) |
| delete a knot | right click or double click; Del — selected knot |
| corner knot | Alt+drag a handle; Alt+click a knot toggles smooth ↔ corner |
| undo / redo | Ctrl+Z / Ctrl+Shift+Z (or the buttons) |
| reset | *Diagonal* |

The green marker shows where the last played note hit the curve. In a DAW the
host may keep keyboard shortcuts for itself (in Reaper: enable *Send all
keyboard input to plugin* for the FX window); the buttons always work.

## VST3 or CLAP?

Prefer **CLAP** when you use hi-res velocity. In VST3, MIDI CCs reach the plugin
as parameter changes: within one sample nih-plug delivers all CCs before the
notes, so in a chord played in a single sample only the first note gets its own
LL, and a host may drop a CC88 that repeats the previous value. CLAP delivers
MIDI events in order and has neither problem.

## Download

Ready builds for Linux (x86_64), macOS (universal: Intel + Apple Silicon) and
Windows (x86_64): [Releases](https://github.com/ScherbakovAl/vst_midi_curves/releases/latest). Each archive holds
`MidiCurves.vst3`, `MidiCurves.clap` and the standalone `midi_curves`; where to
copy them — [BUILD_GUIDE.md](BUILD_GUIDE.md#install). `SHA256SUMS.txt` checks
the download: `sha256sum -c SHA256SUMS.txt --ignore-missing`.

- Linux: glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and later).
- macOS: the build is not signed; after unpacking run
  `xattr -dr com.apple.quarantine MidiCurves.vst3 MidiCurves.clap midi_curves`
  (or allow it in System Settings → Privacy & Security).

## Build

```bash
./build_linux.sh        # Linux: checks dependencies, then ./build.sh
./build.sh --install    # build, install to ~/.vst3 and ~/.clap, start Reaper (flatpak)
./build_macos.sh        # macOS universal binary (VST3 + CLAP bundles)
./build_windows.sh      # Windows (native or via `cross`)
```

Output: `build/midi_curves_v<version>/` with `MidiCurves.vst3`, `MidiCurves.clap`
and the standalone `midi_curves`. Details and dependencies: [BUILD_GUIDE.md](BUILD_GUIDE.md).

## Standalone

`midi_curves` connects one MIDI input to one MIDI output (e.g. keyboard →
*Midi Through* → a synth). Ports are matched by exact name, or by name without
the ALSA address (`… 24:0`) when the device reconnected under another client
number. The chosen pair is remembered and reconnected automatically: the port
lists are re-scanned every 2 s, a port that disappeared closes the connection and
the chosen device is connected as soon as it (re)appears — also when it is plugged
in after start. *Refresh* re-scans and reconnects at once. The app's own ports are
not listed (choosing them would make a loop). Settings live in `~/.config/vst_midi_curves/settings.json`
(old-format files are migrated; the old file is kept as `settings.v1.json`).

## Tests

```bash
cargo test                                                         # all tests
cargo test --release --test realtime -- --include-ignored --nocapture  # + stress, ns/event
cargo build --features assert_process_allocs   # debug plugin that aborts on any allocation in process()
```

Tests never touch the real `~/.config` (temporary folders). See
[ARCHITECTURE.md](ARCHITECTURE.md) for the code map.

## Citation

If you use MIDI Curves in research, please cite it — see
[CITATION.cff](CITATION.cff) (GitHub: *Cite this repository*).

## License

The source code is MIT — see [LICENSE](LICENSE).

Built binaries (the release archives, and builds you make yourself) also contain
nih-plug's VST3 bindings ([vst3-sys](https://github.com/RustAudio/vst3-sys)),
which are GPLv3. So the binaries as a whole are distributed under the
[GNU GPL v3](licenses/LICENSE-GPL-3.0); their source is this repository at the
release tag.
