#!/bin/bash
# Diagnostic script for VST3 plugin verification

echo "🔍 VST3 Plugin MidiCurves Diagnostics"
echo "========================================"
echo ""

VST3_PATH="build/midi_curves_v0.1.0/MidiCurves.vst3"

# 1. Structure verification
echo "1️⃣ VST3 bundle structure verification:"
if [ -d "$VST3_PATH" ]; then
    echo "✅ VST3 bundle exists"
    tree -L 3 "$VST3_PATH"
else
    echo "❌ VST3 bundle not found!"
    exit 1
fi
echo ""

# 2. Binary file verification
echo "2️⃣ Binary file verification:"
BINARY="$VST3_PATH/Contents/x86_64-linux/MidiCurves.so"
if [ -f "$BINARY" ]; then
    echo "✅ Binary file exists"
    echo "   Type: $(file $BINARY | cut -d: -f2)"
    echo "   Size: $(du -h $BINARY | cut -f1)"
    echo "   Permissions: $(ls -l $BINARY | awk '{print $1}')"
else
    echo "❌ Binary file not found!"
    exit 1
fi
echo ""

# 3. Exported symbols verification
echo "3️⃣ VST3 exported symbols verification:"
REQUIRED_SYMBOLS=("GetPluginFactory" "ModuleEntry" "ModuleExit")
MISSING=0

for symbol in "${REQUIRED_SYMBOLS[@]}"; do
    if nm -D "$BINARY" | grep -q " T $symbol"; then
        echo "✅ $symbol"
    else
        echo "❌ $symbol - MISSING!"
        MISSING=1
    fi
done

if [ $MISSING -eq 1 ]; then
    echo ""
    echo "⚠️  WARNING: Required symbols are missing!"
    exit 1
fi
echo ""

# 4. Info.plist verification
echo "4️⃣ Info.plist verification:"
PLIST="$VST3_PATH/Contents/Info.plist"
if [ -f "$PLIST" ]; then
    echo "✅ Info.plist exists"
    echo ""
    echo "Key parameters:"
    echo "  Name: $(grep -A1 '<key>CFBundleName</key>' $PLIST | grep string | sed 's/.*<string>\(.*\)<\/string>.*/\1/')"
    echo "  ID: $(grep -A1 '<key>CFBundleIdentifier</key>' $PLIST | grep string | sed 's/.*<string>\(.*\)<\/string>.*/\1/')"
    echo "  Version: $(grep -A1 '<key>CFBundleVersion</key>' $PLIST | grep string | sed 's/.*<string>\(.*\)<\/string>.*/\1/')"
    echo "  Category: $(grep -A1 '<key>Category</key>' $PLIST | grep string | sed 's/.*<string>\(.*\)<\/string>.*/\1/' | head -1)"
    echo "  SubCategories: $(grep -A1 '<key>SubCategories</key>' $PLIST | grep string | sed 's/.*<string>\(.*\)<\/string>.*/\1/')"
else
    echo "❌ Info.plist not found!"
    exit 1
fi
echo ""

# 5. Dependencies verification
echo "5️⃣ Library dependencies verification:"
echo "Using ldd for verification..."
MISSING_LIBS=$(ldd "$BINARY" | grep "not found" | wc -l)
if [ $MISSING_LIBS -eq 0 ]; then
    echo "✅ All libraries found"
    echo ""
    echo "Main dependencies:"
    ldd "$BINARY" | grep -E "(libGL|libX11|libasound|libgcc|libc\.so)" | sed 's/^/  /'
else
    echo "❌ Missing libraries:"
    ldd "$BINARY" | grep "not found" | sed 's/^/  /'
    exit 1
fi
echo ""

# 6. Reaper verification
echo "6️⃣ Reaper verification:"
if command -v reaper &> /dev/null; then
    echo "✅ Reaper installed: $(which reaper)"
    REAPER_VERSION=$(reaper -v 2>&1 || echo "Failed to determine version")
    echo "   Version: $REAPER_VERSION"
else
    echo "⚠️  Reaper not found in PATH"
fi
echo ""

# 7. Reaper VST3 paths verification
echo "7️⃣ VST3 directories verification:"
VST3_DIRS=(
    "$HOME/.vst3"
    "/usr/lib/vst3"
    "/usr/local/lib/vst3"
)

for dir in "${VST3_DIRS[@]}"; do
    if [ -d "$dir" ]; then
        echo "✅ $dir (exists)"
        if [ -d "$dir/MidiCurves.vst3" ]; then
            echo "   ⚠️  MidiCurves.vst3 is already installed here!"
        fi
    else
        echo "➖ $dir (does not exist)"
    fi
done
echo ""

# 8. Recommendations
echo "📋 Recommendations:"
echo ""
echo "1. Install the plugin:"
echo "   mkdir -p ~/.vst3"
echo "   cp -r $VST3_PATH ~/.vst3/"
echo ""
echo "2. Restart Reaper"
echo ""
echo "3. In Reaper:"
echo "   - Options -> Preferences -> Plug-ins -> VST"
echo "   - Click 'Re-scan' to re-scan"
echo "   - Check 'Clear cache/re-scan' if plugin doesn't appear"
echo ""
echo "4. Check Reaper log at:"
echo "   ~/.config/REAPER/reaper.ini (settings)"
echo "   ~/.config/REAPER/reaper-vstplugins64.ini (plugin cache)"
echo ""

# 9. Attempt to find Reaper error logs
echo "9️⃣ Reaper log search:"
REAPER_LOG="$HOME/.config/REAPER/reaper-vstplugins64.ini"
if [ -f "$REAPER_LOG" ]; then
    echo "✅ Plugin cache found: $REAPER_LOG"
    if grep -q "MidiCurves" "$REAPER_LOG"; then
        echo ""
        echo "MidiCurves information in cache:"
        grep -A5 "MidiCurves" "$REAPER_LOG" | sed 's/^/  /'
    else
        echo "   ℹ️  MidiCurves not found in cache (plugin hasn't been scanned yet)"
    fi
else
    echo "➖ Plugin cache not found"
fi
echo ""

echo "✅ Diagnostics completed!"
echo ""
echo "💡 If plugin still doesn't load in Reaper:"
echo "   1. Check for conflicts with other plugins"
echo "   2. Try running Reaper from terminal to view errors:"
echo "      reaper 2>&1 | tee reaper_log.txt"
echo "   3. Ensure Reaper has read permissions for VST3 folder"