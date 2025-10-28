#!/bin/bash
# Build script for macOS (Universal Binary: Intel + Apple Silicon)
# Requires installed Rust and XCode Command Line Tools

set -e

echo "🍎 Starting VST3 plugin build for macOS..."

# Check for Rust
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust is not installed! Install from https://rustup.rs/"
    exit 1
fi

# Check for XCode Command Line Tools
if ! xcode-select -p &> /dev/null; then
    echo "❌ XCode Command Line Tools are not installed!"
    echo "Install with: xcode-select --install"
    exit 1
fi

echo "🏗️ Building VST3 plugin for Intel..."
cargo build --release --target x86_64-apple-darwin --bin midi_curves_vst3

echo "🏗️ Building VST3 plugin for Apple Silicon..."
cargo build --release --target aarch64-apple-darwin --bin midi_curves_vst3

echo "🏗️ Building standalone application for Intel..."
cargo build --release --target x86_64-apple-darwin --bin midi_curves

echo "🏗️ Building standalone application for Apple Silicon..."
cargo build --release --target aarch64-apple-darwin --bin midi_curves

echo "🔗 Creating Universal Binary..."
# Create universal binary for VST3
lipo -create \
    "target/x86_64-apple-darwin/release/midi_curves_vst3" \
    "target/aarch64-apple-darwin/release/midi_curves_vst3" \
    -output "target/universal/midi_curves_vst3"

# Create universal binary for standalone
lipo -create \
    "target/x86_64-apple-darwin/release/midi_curves" \
    "target/aarch64-apple-darwin/release/midi_curves" \
    -output "target/universal/midi_curves"

echo "📁 Creating VST3 structure for macOS..."
mkdir -p "target/universal/MidiCurves.vst3/Contents/MacOS"
cp "target/universal/midi_curves_vst3" "target/universal/MidiCurves.vst3/Contents/MacOS/MidiCurves"

# Create Info.plist for VST3
cat > "target/universal/MidiCurves.vst3/Contents/Info.plist" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>MidiCurves</string>
    <key>CFBundleIdentifier</key>
    <string>com.yourcompany.midicurves</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>MIDI Curves</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>CFBundleSupportedPlatforms</key>
    <array>
        <string>MacOSX</string>
    </array>
    <key>CSResourcesFileMapped</key>
    <true/>
</dict>
</plist>
EOF

# Create PkgInfo
echo -n "BNDL????" > "target/universal/MidiCurves.vst3/Contents/PkgInfo"

# Set permissions
chmod -R 755 "target/universal/MidiCurves.vst3"
chmod 755 "target/universal/MidiCurves.vst3/Contents/MacOS/MidiCurves"
chmod 755 "target/universal/midi_curves"

echo "✅ Build completed!"
echo "📂 Files are located in:"
echo "   - VST3: target/universal/MidiCurves.vst3"
echo "   - Standalone: target/universal/midi_curves"

# Installation instructions
echo ""
echo "📋 Installation instructions:"
echo "VST3:"
echo "   1. Copy the MidiCurves.vst3 folder to"
echo "      /Library/Audio/Plug-Ins/VST3/ (for all users)"
echo "   or"
echo "      ~/Library/Audio/Plug-Ins/VST3/ (for current user only)"
echo ""
echo "Standalone:"
echo "   1. Move midi_curves to Applications folder"
echo "   2. Allow in Security & Privacy on first launch"
echo ""
echo "🔧 Troubleshooting:"
echo "If you get 'developer cannot be verified' error:"
echo "   1. Open System Preferences > Security & Privacy"
echo "   2. Click 'Allow Anyway' for the application"