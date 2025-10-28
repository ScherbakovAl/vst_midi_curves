#!/bin/bash
# Build script for Linux (x86_64)
# Requires installed Rust and necessary system libraries

set -e

echo "🐧 Starting VST3 plugin build for Linux..."

# Check for Rust
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust is not installed! Install from https://rustup.rs/"
    exit 1
fi

# Check for necessary build libraries
echo "📦 Checking system dependencies..."

# Check for pkg-config
if ! command -v pkg-config &> /dev/null; then
    echo "⚠️  pkg-config not found. Install via package manager:"
    echo "   Ubuntu/Debian: sudo apt install pkg-config"
    echo "   Fedora: sudo dnf install pkg-config"
    echo "   Arch: sudo pacman -S pkg-config"
fi

# Check for GTK headers (for eframe)
if ! pkg-config --exists gtk4; then
    echo "⚠️  GTK4 not found. Install via package manager:"
    echo "   Ubuntu/Debian: sudo apt install libgtk-4-dev"
    echo "   Fedora: sudo dnf install gtk4-devel"
    echo "   Arch: sudo pacman -S gtk4"
fi

echo "🏗️ Building VST3 plugin..."
cargo build --release --lib

echo "📦 Building standalone application..."
cargo build --release --bin midi_curves

echo "📁 Creating VST3 structure for Linux..."
mkdir -p "target/release/vst3/x86_64-linux"
cp "target/release/libvst_midi_curves.so" "target/release/vst3/x86_64-linux/MidiCurves.so"

# Create desktop file for standalone application
mkdir -p "target/release/applications"
cat > "target/release/applications/MidiCurves.desktop" << EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=MIDI Curves
Comment=VST3 plugin for MIDI velocity processing
Exec=$PWD/target/release/midi_curves
Icon=applications-multimedia
Terminal=false
Categories=AudioVideo;Audio;
Keywords=vst3;midi;plugin;daw;
MimeType=application/x-vst3-plugin;
EOF

# Install desktop file
echo "📄 Installing desktop file..."
if command -v xdg-desktop-menu &> /dev/null; then
    xdg-desktop-menu install --mode system "target/release/applications/MidiCurves.desktop"
fi

echo "✅ Build completed!"
echo "📂 Files are located in:"
echo "   - VST3: target/release/vst3/x86_64-linux/MidiCurves.so"
echo "   - Standalone: target/release/midi_curves"
echo "   - Desktop file: target/release/applications/MidiCurves.desktop"

# Installation instructions
echo ""
echo "📋 Installation instructions:"
echo "VST3:"
echo "   1. Copy the MidiCurves.vst3 folder to"
echo "      ~/.vst3/ (for current user)"
echo "   or"
echo "      /usr/lib/vst3/ (for all users, requires sudo)"
echo ""
echo "Standalone:"
echo "   1. Make sure you have dependencies installed:"
echo "      sudo apt install libgtk-4-1 libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libspeechd-dev libxkbcommon-dev libssl-dev libasound2-dev libudev-dev libcairo2-dev libgdk-pixbuf2.0-dev"
echo "   2. Run the application:"
echo "      ./target/release/midi_curves"
echo ""
echo "🔧 Troubleshooting:"
echo "If build errors occur:"
echo "   1. Update Rust: rustup update"
echo "   2. Clean build: cargo clean"
echo "   3. Rebuild: cargo build --release"