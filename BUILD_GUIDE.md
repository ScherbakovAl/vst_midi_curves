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
not from this crate. CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml))
runs clippy with `-D warnings` and allows only this lint
(`-A semicolon_in_expressions_from_non_local_macros`), then `cargo test`, on
Linux, Windows and macOS.

## Releases

Built files go to GitHub Releases, not into git — `build/` is ignored. Releases
are built by GitHub Actions ([`.github/workflows/release.yml`](.github/workflows/release.yml)),
not on a developer machine: a local Linux build needs the glibc of the
distribution it was built on (Fedora 44 — 2.43), the CI build on Ubuntu 22.04
needs only 2.35.

1. Set `version` in `Cargo.toml` (and `version`/`date-released` in
   `CITATION.cff`), commit.
2. `git tag v<version> && git push origin v<version>` — the tag must equal the
   version, otherwise the workflow stops.
3. The workflow runs `build.sh`, `build_macos.sh` and `build_windows.sh` on their
   platforms and attaches `MidiCurves-v<version>-{linux-x86_64.tar.gz,
   macos-universal.zip, windows-x86_64.zip}` and `SHA256SUMS.txt` to a **draft**
   release.
4. Check the draft and press *Publish release*. Zenodo archives only published
   releases and gives each one a DOI.

Windows builds link the C runtime statically ([`.cargo/config.toml`](.cargo/config.toml)):
no Visual C++ Redistributable is needed.

Every archive also holds `LICENSE-GPL-3.0`: the binaries contain nih-plug's
GPLv3 VST3 bindings (see *License* in README).
