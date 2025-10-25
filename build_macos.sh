#!/bin/bash
# Скрипт сборки для macOS (Universal Binary: Intel + Apple Silicon)
# Требует установленного Rust и XCode Command Line Tools

set -e

echo "🍎 Начало сборки VST3 плагина для macOS..."

# Проверяем наличие Rust
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust не установлен! Установите с https://rustup.rs/"
    exit 1
fi

# Проверяем наличие XCode Command Line Tools
if ! xcode-select -p &> /dev/null; then
    echo "❌ XCode Command Line Tools не установлены!"
    echo "Установите с помощью: xcode-select --install"
    exit 1
fi

echo "🏗️ Сборка VST3 плагина для Intel..."
cargo build --release --target x86_64-apple-darwin --bin midi_curves_vst3

echo "🏗️ Сборка VST3 плагина для Apple Silicon..."
cargo build --release --target aarch64-apple-darwin --bin midi_curves_vst3

echo "🏗️ Сборка standalone приложения для Intel..."
cargo build --release --target x86_64-apple-darwin --bin midi_curves

echo "🏗️ Сборка standalone приложения для Apple Silicon..."
cargo build --release --target aarch64-apple-darwin --bin midi_curves

echo "🔗 Создание Universal Binary..."
# Создаем universal binary для VST3
lipo -create \
    "target/x86_64-apple-darwin/release/midi_curves_vst3" \
    "target/aarch64-apple-darwin/release/midi_curves_vst3" \
    -output "target/universal/midi_curves_vst3"

# Создаем universal binary для standalone
lipo -create \
    "target/x86_64-apple-darwin/release/midi_curves" \
    "target/aarch64-apple-darwin/release/midi_curves" \
    -output "target/universal/midi_curves"

echo "📁 Создание структуры VST3 для macOS..."
mkdir -p "target/universal/MidiCurves.vst3/Contents/MacOS"
cp "target/universal/midi_curves_vst3" "target/universal/MidiCurves.vst3/Contents/MacOS/MidiCurves"

# Создаем Info.plist для VST3
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

# Создаем PkgInfo
echo -n "BNDL????" > "target/universal/MidiCurves.vst3/Contents/PkgInfo"

# Устанавливаем права доступа
chmod -R 755 "target/universal/MidiCurves.vst3"
chmod 755 "target/universal/MidiCurves.vst3/Contents/MacOS/MidiCurves"
chmod 755 "target/universal/midi_curves"

echo "✅ Сборка завершена!"
echo "📂 Файлы находятся в:"
echo "   - VST3: target/universal/MidiCurves.vst3"
echo "   - Standalone: target/universal/midi_curves"

# Инструкции по установке
echo ""
echo "📋 Инструкции по установке:"
echo "VST3:"
echo "   1. Скопируйте папку MidiCurves.vst3 в"
echo "      /Library/Audio/Plug-Ins/VST3/ (для всех пользователей)"
echo "   или"
echo "      ~/Library/Audio/Plug-Ins/VST3/ (только для текущего пользователя)"
echo ""
echo "Standalone:"
echo "   1. Переместите midi_curves в папку Applications"
echo "   2. При первом запуске разрешите в Security & Privacy"
echo ""
echo "🔧 Устранение проблем:"
echo "Если получаете ошибку 'невозможно проверить разработчика':"
echo "   1. Откройте System Preferences > Security & Privacy"
echo "   2. Нажмите 'Allow Anyway' для приложения"