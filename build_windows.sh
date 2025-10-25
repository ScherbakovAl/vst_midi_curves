#!/bin/bash
# Скрипт сборки для Windows (x86_64)
# Требует установленного Rust и cross для кросс-компиляции

set -e

echo "🔨 Начало сборки VST3 плагина для Windows..."

# Проверяем наличие Rust
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust не установлен! Установите с https://rustup.rs/"
    exit 1
fi

# Устанавливаем cross если его нет
if ! command -v cross &> /dev/null; then
    echo "📦 Установка cross для кросс-компиляции..."
    cargo install cross
fi

echo "🏗️ Сборка VST3 плагина..."
cross build --release --target x86_64-pc-windows-msvc

echo "📁 Создание структуры VST3..."
mkdir -p "target/x86_64-pc-windows-msvc/release/vst3"
cp "target/x86_64-pc-windows-msvc/release/midi_curves_vst3.dll" "target/x86_64-pc-windows-msvc/release/vst3/MidiCurves.vst3"

echo "📦 Создание standalone приложения..."
cross build --release --target x86_64-pc-windows-msvc --bin midi_curves
cp "target/x86_64-pc-windows-msvc/release/midi_curves.exe" "target/x86_64-pc-windows-msvc/release/"

echo "✅ Сборка завершена!"
echo "📂 Файлы находятся в:"
echo "   - VST3: target/x86_64-pc-windows-msvc/release/vst3/MidiCurves.vst3"
echo "   - Standalone: target/x86_64-pc-windows-msvc/release/midi_curves.exe"

# Инструкции по установке
echo ""
echo "📋 Инструкции по установке:"
echo "VST3:"
echo "   1. Скопируйте папку MidiCurves.vst3 в"
echo "      C:\\Program Files\\Common Files\\VST3\\"
echo ""
echo "Standalone:"
echo "   1. Скопируйте midi_curves.exe в желаемую папку"
echo "   2. Запустите приложение"