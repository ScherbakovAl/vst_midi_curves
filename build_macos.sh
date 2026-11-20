#!/bin/bash
# Сборка для macOS: universal binary (Intel + Apple Silicon), бандлы VST3 и
# CLAP, standalone. Нужны Rust с целями x86_64/aarch64-apple-darwin и Xcode
# Command Line Tools:
#   rustup target add x86_64-apple-darwin aarch64-apple-darwin
#   xcode-select --install

set -e
cd "$(dirname "$0")"

if ! command -v cargo &> /dev/null; then
    echo "❌ Rust is not installed! Install from https://rustup.rs/"
    exit 1
fi
if ! xcode-select -p &> /dev/null; then
    echo "❌ Xcode Command Line Tools are not installed: xcode-select --install"
    exit 1
fi

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OUT="build/midi_curves_v$VERSION"
BUNDLE_ID="io.github.scherbakoval.midi-curves"

for t in x86_64-apple-darwin aarch64-apple-darwin; do
    echo "🏗️  $t"
    cargo build --release --locked --target "$t" --lib --bin midi_curves
done

rm -rf "$OUT/MidiCurves.vst3" "$OUT/MidiCurves.clap"
mkdir -p "$OUT"
lipo -create \
    target/x86_64-apple-darwin/release/libvst_midi_curves.dylib \
    target/aarch64-apple-darwin/release/libvst_midi_curves.dylib \
    -output "$OUT/MidiCurves.dylib"
lipo -create \
    target/x86_64-apple-darwin/release/midi_curves \
    target/aarch64-apple-darwin/release/midi_curves \
    -output "$OUT/midi_curves"

# Бандл: <имя>.<ext>/Contents/{Info.plist,PkgInfo,MacOS/MidiCurves}
make_bundle() {
    local ext="$1"
    local dir="$OUT/MidiCurves.$ext/Contents"
    mkdir -p "$dir/MacOS"
    cp "$OUT/MidiCurves.dylib" "$dir/MacOS/MidiCurves"
    cat > "$dir/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>MidiCurves</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID.$ext</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>MIDI Curves</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>CSResourcesFileMapped</key>
    <true/>
</dict>
</plist>
EOF
    echo -n "BNDL????" > "$dir/PkgInfo"
}
make_bundle vst3
make_bundle clap
rm "$OUT/MidiCurves.dylib"
cp README.md BUILD_GUIDE.md LICENSE licenses/LICENSE-GPL-3.0 "$OUT/"

echo "✅ $OUT/"
ls -1 "$OUT"
echo ""
echo "Install (current user):"
echo "   cp -r $OUT/MidiCurves.vst3 ~/Library/Audio/Plug-Ins/VST3/"
echo "   cp -r $OUT/MidiCurves.clap ~/Library/Audio/Plug-Ins/CLAP/"
echo "Unsigned build: on first launch allow it in System Settings → Privacy & Security."
