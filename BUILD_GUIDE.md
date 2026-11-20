# Building MIDI Curves

One crate, two targets:

- **lib** `vst_midi_curves` (`cdylib` + `rlib`) — the plugin; one binary exports
  both VST3 (`GetPluginFactory`) and CLAP (`clap_entry`);
- **bin** `midi_curves` — the standalone app.

`Cargo.lock` is committed: it pins nih-plug to `28b149e` (also written as `rev`
in `Cargo.toml`). Build with `--locked`.

## Requirements

- Rust (stable, edition 2021): <https://rustup.rs/>
- **Linux** — pkg-config and headers of ALSA, X11/xcb and OpenGL (no GTK, no JACK):
  - Fedora: `sudo dnf install pkg-config alsa-lib-devel libX11-devel libxcb-devel libglvnd-devel`
  - Ubuntu/Debian: `sudo apt install pkg-config libasound2-dev libx11-dev libx11-xcb-dev libxcb1-dev libgl-dev`
  - Arch: `sudo pacman -S pkgconf alsa-lib libx11 libxcb libglvnd`
- **macOS** — Xcode Command Line Tools (`xcode-select --install`) and both targets:
  `rustup target add x86_64-apple-darwin aarch64-apple-darwin`
- **Windows** — MSVC toolchain when building natively; from Linux — `cross`
  (Docker/Podman) with the `x86_64-pc-windows-gnu` target.

## Scripts

| Script | Result |
|---|---|
| `./build_linux.sh` | checks Linux dependencies, runs `build.sh`, writes `MidiCurves.desktop` (install it with `xdg-desktop-menu install --mode user …`) |
| `./build.sh` | Linux: `build/midi_curves_v<version>/` — `MidiCurves.vst3/`, `MidiCurves.clap`, `midi_curves`; on macOS it runs `build_macos.sh` |
| `./build.sh --install` | the same, then installs to `~/.vst3` and `~/.clap` and starts Reaper (flatpak) |
| `./build_macos.sh` | universal binary; `MidiCurves.vst3` and `MidiCurves.clap` bundles, `midi_curves` |
| `./build_windows.sh` | `build/midi_curves_v<version>_windows/` — `MidiCurves.vst3/`, `MidiCurves.clap`, `midi_curves.exe` |
| `./test_vst3_load.sh` | checks the built Linux VST3 bundle: structure, exported symbols, libraries |

## Manual build

```bash
cargo build --release --locked --lib            # target/release/libvst_midi_curves.so (.dylib / .dll)
cargo build --release --locked --bin midi_curves
```

Bundle layout:

| Platform | VST3 | CLAP |
|---|---|---|
| Linux | `MidiCurves.vst3/Contents/x86_64-linux/MidiCurves.so` | the `.so` renamed to `MidiCurves.clap` |
| macOS | `MidiCurves.vst3/Contents/MacOS/MidiCurves` + `Info.plist` | `MidiCurves.clap/Contents/MacOS/MidiCurves` + `Info.plist` |
| Windows | `MidiCurves.vst3/Contents/x86_64-win/MidiCurves.vst3` (the dll) | the dll renamed to `MidiCurves.clap` |

## Install

| Platform | VST3 | CLAP |
|---|---|---|
| Linux | `~/.vst3/` | `~/.clap/` |
| macOS | `~/Library/Audio/Plug-Ins/VST3/` | `~/Library/Audio/Plug-Ins/CLAP/` |
| Windows | `C:\Program Files\Common Files\VST3\` | `C:\Program Files\Common Files\CLAP\` |

In Reaper: *Options → Preferences → Plug-ins → VST/CLAP → Re-scan*. Insert the
plugin on a track before the instrument. MIDI Curves has no parameters to
automate: the curves are stored in the project.

The VST3 class ID (`MidiCurvesVST3!!`) never changes, so projects made with
older builds still find the plugin; their curves start linear (older builds did
not save curves in the project).

## Checks

```bash
cargo test                                                              # unit + integration
cargo test --release --test realtime -- --include-ignored --nocapture  # stress test + ns/event
cargo build --features assert_process_allocs   # debug plugin: aborts on allocation in process()
cargo clippy --all-targets
```

The only build warning, “trailing semicolon in macro used in expression
position”, comes from the `vst3_com` macro inside `nih_export_vst3!` (nih-plug),
not from this crate.

## Releases

Built files go to GitHub Releases (attach the archive of
`build/midi_curves_v<version>/`), not into git — `build/` is ignored.
