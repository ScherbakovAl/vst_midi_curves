#!/bin/bash
# Сборка для Windows (x86_64): на Windows (Git Bash/MSYS) — обычный cargo,
# на Linux — кросс-сборка через cross (нужен Docker/Podman).

set -e
cd "$(dirname "$0")"

TARGET=x86_64-pc-windows-msvc
if [ "$OS" = "Windows_NT" ]; then
    CARGO=cargo
else
    TARGET=x86_64-pc-windows-gnu
    command -v cross &> /dev/null || cargo install cross
    CARGO=cross
fi

$CARGO build --release --locked --target "$TARGET" --lib --bin midi_curves

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OUT="build/midi_curves_v${VERSION}_windows"
REL="target/$TARGET/release"
rm -rf "$OUT/MidiCurves.vst3" "$OUT/MidiCurves.clap"
mkdir -p "$OUT/MidiCurves.vst3/Contents/x86_64-win"
# VST3: бандл-папка с .vst3 внутри; CLAP: та же dll с расширением .clap.
cp "$REL/vst_midi_curves.dll" "$OUT/MidiCurves.vst3/Contents/x86_64-win/MidiCurves.vst3"
cp "$REL/vst_midi_curves.dll" "$OUT/MidiCurves.clap"
cp "$REL/midi_curves.exe" "$OUT/"
cp README.md BUILD_GUIDE.md LICENSE "$OUT/"

echo "✅ $OUT/"
echo "Install: MidiCurves.vst3 → C:\\Program Files\\Common Files\\VST3\\"
echo "         MidiCurves.clap → C:\\Program Files\\Common Files\\CLAP\\"
