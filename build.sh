#!/bin/bash
# Сборка MIDI Curves: VST3 + CLAP + standalone в build/midi_curves_v<версия>/.
# Linux — здесь; macOS — делегируется в build_macos.sh (universal binary).
#
#   ./build.sh            собрать
#   ./build.sh --install  собрать, поставить в ~/.vst3 и ~/.clap и запустить
#                         Reaper (flatpak) — для проверки

set -e
cd "$(dirname "$0")"

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OUT="build/midi_curves_v$VERSION"
PLATFORM=$(uname -s)

echo "🎵 MIDI Curves $VERSION — $PLATFORM"

if [ "$PLATFORM" = "Darwin" ]; then
    exec ./build_macos.sh "$@"
fi
if [ "$PLATFORM" != "Linux" ]; then
    echo "⚠️  Unsupported platform: $PLATFORM (Windows: build_windows.sh)"
    exit 1
fi

cargo build --release --locked --lib --bin midi_curves

LIB="target/release/libvst_midi_curves.so"
rm -rf "$OUT/MidiCurves.vst3" "$OUT/MidiCurves.clap"
mkdir -p "$OUT/MidiCurves.vst3/Contents/x86_64-linux"
# VST3: бандл-папка; CLAP: та же .so с расширением .clap.
cp "$LIB" "$OUT/MidiCurves.vst3/Contents/x86_64-linux/MidiCurves.so"
cp Info.plist.linux "$OUT/MidiCurves.vst3/Contents/Info.plist"
cp "$LIB" "$OUT/MidiCurves.clap"
cp target/release/midi_curves "$OUT/"
cp README.md BUILD_GUIDE.md LICENSE licenses/LICENSE-GPL-3.0 "$OUT/"

echo "✅ $OUT/"
ls -1 "$OUT"

if [ "$1" = "--install" ]; then
    mkdir -p ~/.vst3 ~/.clap
    rm -rf ~/.vst3/MidiCurves.vst3
    cp -r "$OUT/MidiCurves.vst3" ~/.vst3/
    cp "$OUT/MidiCurves.clap" ~/.clap/
    echo "📦 Installed: ~/.vst3/MidiCurves.vst3, ~/.clap/MidiCurves.clap"
    flatpak run fm.reaper.Reaper
else
    echo ""
    echo "Install: copy MidiCurves.vst3 to ~/.vst3/ and/or MidiCurves.clap to ~/.clap/"
    echo "         (or ./build.sh --install)"
fi
