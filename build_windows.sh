#!/bin/bash
# Build script for Windows (x86_64)
# Requires installed Rust and cross for cross-compilation

set -e

echo "🔨 Starting VST3 plugin build for Windows..."

# Check for Rust presence
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust is not installed! Install from https://rustup.rs/"
    exit 1
fi

# Install cross if it's not available
if ! command -v cross &> /dev/null; then
    echo "📦 Installing cross for cross-compilation..."
    cargo install cross
fi

echo "🏗️ Building VST3 plugin..."
cross build --release --target x86_64-pc-windows-msvc

echo "📁 Creating VST3 structure..."
mkdir -p "target/x86_64-pc-windows-msvc/release/vst3"
cp "target/x86_64-pc-windows-msvc/release/midi_curves_vst3.dll" "target/x86_64-pc-windows-msvc/release/vst3/MidiCurves.vst3"

echo "📦 Creating standalone application..."
cross build --release --target x86_64-pc-windows-msvc --bin midi_curves
cp "target/x86_64-pc-windows-msvc/release/midi_curves.exe" "target/x86_64-pc-windows-msvc/release/"

echo "✅ Build completed!"
echo "📂 Files are located in:"
echo "   - VST3: target/x86_64-pc-windows-msvc/release/vst3/MidiCurves.vst3"
echo "   - Standalone: target/x86_64-pc-windows-msvc/release/midi_curves.exe"

# Installation instructions
echo ""
echo "📋 Installation instructions:"
echo "VST3:"
echo "   1. Copy the MidiCurves.vst3 folder to"
echo "      C:\\Program Files\\Common Files\\VST3\\"
echo ""
echo "Standalone:"
echo "   1. Copy midi_curves.exe to desired folder"
echo "   2. Launch the application"