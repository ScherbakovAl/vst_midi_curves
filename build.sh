#!/bin/bash

# Build Script for VST3 MIDI Curves
# Automated build script for Linux, macOS and Windows

set -e

echo "🎵 VST3 MIDI Curves - Build Script"
echo "=================================="

# Checking platform
PLATFORM=$(uname -s)
echo "🔍 Platform detected: $PLATFORM"

case $PLATFORM in
    "Linux")
        echo "🐧 Building for Linux..."
        cargo build --release --lib
        echo "✅ Linux build completed!"
        ;;
    "Darwin")
        echo "🍎 Building for macOS..."
                
                # Building VST3 library only
                cargo build --release --lib
                
                echo "✅ macOS build completed!"
        ;;
    *)
        echo "⚠️  Unsupported platform: $PLATFORM"
        echo "💡 Supported platforms: Linux, macOS, Windows"
        exit 1
        ;;
esac

echo ""
echo "📦 Creating distribution..."

# Creating build directory
mkdir -p build/midi_curves_v0.1.0

case $PLATFORM in
    "Linux")
        # Linux build - VST3 plugin only
        
        # Creating proper VST3 plugin structure
        mkdir -p build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/x86_64-linux
        
        # Checking for VST3 plugin presence
        VST3_PLUGIN="target/release/deps/libvst_midi_curves.so"
        if [ -f "$VST3_PLUGIN" ]; then
            cp "$VST3_PLUGIN" build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/x86_64-linux/MidiCurves.so
            echo "✅ VST3 plugin copied"
        else
            echo "❌ VST3 plugin not found: $VST3_PLUGIN"
            exit 1
        fi
        
        # Copying Info.plist for Linux
        if [ -f "Info.plist.linux" ]; then
            cp Info.plist.linux build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/Info.plist
        else
            echo "❌ Info.plist.linux not found"
            exit 1
        fi
        
        # Creating directory for resources
        mkdir -p build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/Resources
        
        cp README.md build/midi_curves_v0.1.0/ 2>/dev/null || echo "📄 README.md not found"
        cp BUILD_GUIDE.md build/midi_curves_v0.1.0/ 2>/dev/null || echo "📄 BUILD_GUIDE.md not found"
        
        echo "🐧 Linux distribution created: build/midi_curves_v0.1.0/"
        echo "📝 VST3 plugin created as MidiCurves.vst3/"
        ;;
    "Darwin")
        # macOS build - VST3 plugin only
        
        # Creating proper VST3 plugin structure for macOS
        mkdir -p build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/x86_64-darwin
        mkdir -p build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/aarch64-darwin
        mkdir -p build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/Resources
        
        # For simplicity, copying same files for both architectures
        VST3_PLUGIN="target/release/deps/libvst_midi_curves.dylib"
        if [ -f "$VST3_PLUGIN" ]; then
            cp "$VST3_PLUGIN" build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/x86_64-darwin/MidiCurves
            cp "$VST3_PLUGIN" build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/aarch64-darwin/MidiCurves
            echo "✅ VST3 plugin copied for macOS"
        else
            echo "❌ VST3 plugin not found for macOS"
            exit 1
        fi
        
        # Copying Info.plist for macOS
        if [ -f "Info.plist.linux" ]; then
            cp Info.plist.linux build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/Info.plist
        else
            echo "❌ Info.plist.linux not found"
            exit 1
        fi
        
        cp README.md build/midi_curves_v0.1.0/ 2>/dev/null || echo "📄 README.md not found"
        cp BUILD_GUIDE.md build/midi_curves_v0.1.0/ 2>/dev/null || echo "📄 BUILD_GUIDE.md not found"
        
        echo "🍎 macOS distribution created: build/midi_curves_v0.1.0/"
        ;;
esac

echo ""
echo "📋 Build information:"
echo "- VST3 plugin: $(ls -d build/midi_curves_v0.1.0/*.vst3 2>/dev/null || echo "not found")"

echo ""
echo "🎉 Build completed successfully!"
echo "📁 Distribution located at: build/midi_curves_v0.1.0/"
echo ""
echo "📝 For installation:"
echo "- Linux: copy MidiCurves.vst3 to ~/.vst3/"
echo "- macOS: copy MidiCurves.vst3 to ~/Library/Audio/Plug-Ins/VST3/"
echo "- For Reaper: place in VST3 folder specified in settings"

## for test
rm -rf /home/sche/.vst3/MidiCurves.vst3
cp -rf /home/sche/programming/vst_midi_curves/build/midi_curves_v0.1.0/MidiCurves.vst3 /home/sche/.vst3/
flatpak run fm.reaper.Reaper
р