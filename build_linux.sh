#!/bin/bash
# Сборка для Linux (x86_64): проверка зависимостей, затем build.sh.
# Нужны: Rust, pkg-config, заголовки ALSA, X11/xcb, OpenGL (GTK и JACK не нужны).

set -e
cd "$(dirname "$0")"

if ! command -v cargo &> /dev/null; then
    echo "❌ Rust is not installed! Install from https://rustup.rs/"
    exit 1
fi

missing=()
command -v pkg-config &> /dev/null || missing+=(pkg-config)
for pc in alsa x11 xcb gl; do
    pkg-config --exists "$pc" 2> /dev/null || missing+=("$pc")
done
if [ ${#missing[@]} -ne 0 ]; then
    echo "⚠️  Missing: ${missing[*]}. Install:"
    echo "   Fedora:        sudo dnf install pkg-config alsa-lib-devel libX11-devel libxcb-devel libglvnd-devel"
    echo "   Ubuntu/Debian: sudo apt install pkg-config libasound2-dev libx11-dev libx11-xcb-dev libxcb1-dev libgl-dev"
    echo "   Arch:          sudo pacman -S pkgconf alsa-lib libx11 libxcb libglvnd"
    exit 1
fi

./build.sh "$@"

# Ярлык для меню — только для текущего пользователя (без root):
#   xdg-desktop-menu install --mode user MidiCurves.desktop
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OUT="build/midi_curves_v$VERSION"
cat > "$OUT/MidiCurves.desktop" << EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=MIDI Curves
Comment=MIDI velocity curves (standalone)
Exec=$PWD/$OUT/midi_curves
Icon=applications-multimedia
Terminal=false
Categories=AudioVideo;Audio;Midi;
EOF
echo "📄 $OUT/MidiCurves.desktop (install: xdg-desktop-menu install --mode user $OUT/MidiCurves.desktop)"
