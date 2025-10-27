#!/bin/bash
# Скрипт сборки для Linux (x86_64)
# Требует установленного Rust и необходимых системных библиотек

set -e

echo "🐧 Начало сборки VST3 плагина для Linux..."

# Проверяем наличие Rust
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust не установлен! Установите с https://rustup.rs/"
    exit 1
fi

# Проверяем наличие необходимых библиотек для сборки
echo "📦 Проверка системных зависимостей..."

# Проверяем наличие pkg-config
if ! command -v pkg-config &> /dev/null; then
    echo "⚠️  pkg-config не найден. Установите через пакетный менеджер:"
    echo "   Ubuntu/Debian: sudo apt install pkg-config"
    echo "   Fedora: sudo dnf install pkg-config"
    echo "   Arch: sudo pacman -S pkg-config"
fi

# Проверяем наличие GTK headers (для eframe)
if ! pkg-config --exists gtk4; then
    echo "⚠️  GTK4 не найден. Установите через пакетный менеджер:"
    echo "   Ubuntu/Debian: sudo apt install libgtk-4-dev"
    echo "   Fedora: sudo dnf install gtk4-devel"
    echo "   Arch: sudo pacman -S gtk4"
fi

echo "🏗️ Сборка VST3 плагина..."
cargo build --release --lib

echo "📦 Сборка standalone приложения..."
cargo build --release --bin midi_curves

echo "📁 Создание структуры VST3 для Linux..."
mkdir -p "target/release/vst3/x86_64-linux"
cp "target/release/libvst_midi_curves.so" "target/release/vst3/x86_64-linux/MidiCurves.so"

# Создаем desktop файл для standalone приложения
mkdir -p "target/release/applications"
cat > "target/release/applications/MidiCurves.desktop" << EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=MIDI Curves
Comment=VST3 плагин для обработки MIDI velocity
Exec=$PWD/target/release/midi_curves
Icon=applications-multimedia
Terminal=false
Categories=AudioVideo;Audio;
Keywords=vst3;midi;plugin;daw;
MimeType=application/x-vst3-plugin;
EOF

# Устанавливаем desktop файл
echo "📄 Установка desktop файла..."
if command -v xdg-desktop-menu &> /dev/null; then
    xdg-desktop-menu install --mode system "target/release/applications/MidiCurves.desktop"
fi

echo "✅ Сборка завершена!"
echo "📂 Файлы находятся в:"
echo "   - VST3: target/release/vst3/x86_64-linux/MidiCurves.so"
echo "   - Standalone: target/release/midi_curves"
echo "   - Desktop файл: target/release/applications/MidiCurves.desktop"

# Инструкции по установке
echo ""
echo "📋 Инструкции по установке:"
echo "VST3:"
echo "   1. Скопируйте папку MidiCurves.vst3 в"
echo "      ~/.vst3/ (для текущего пользователя)"
echo "   или"
echo "      /usr/lib/vst3/ (для всех пользователей, требует sudo)"
echo ""
echo "Standalone:"
echo "   1. Убедитесь что у вас установлены зависимости:"
echo "      sudo apt install libgtk-4-1 libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libspeechd-dev libxkbcommon-dev libssl-dev libasound2-dev libudev-dev libcairo2-dev libgdk-pixbuf2.0-dev"
echo "   2. Запустите приложение:"
echo "      ./target/release/midi_curves"
echo ""
echo "🔧 Устранение проблем:"
echo "Если возникают ошибки сборки:"
echo "   1. Обновите Rust: rustup update"
echo "   2. Очистите сборку: cargo clean"
echo "   3. Пересоберите: cargo build --release"