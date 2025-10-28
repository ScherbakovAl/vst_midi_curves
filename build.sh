#!/bin/bash

# Build Script для VST3 MIDI Curves
# Скрипт автоматической сборки для Linux, macOS и Windows

set -e

echo "🎵 VST3 MIDI Curves - Скрипт сборки"
echo "=================================="

# Проверяем платформу
PLATFORM=$(uname -s)
echo "🔍 Обнаружена платформа: $PLATFORM"

case $PLATFORM in
    "Linux")
        echo "🐧 Сборка для Linux..."
        cargo build --release --lib
        echo "✅ Сборка для Linux завершена!"
        ;;
    "Darwin")
        echo "🍎 Сборка для macOS..."
        
        # Собираем только библиотеку VST3
        cargo build --release --lib
        
        echo "✅ Сборка для macOS завершена!"
        ;;
    *)
        echo "⚠️  Неподдерживаемая платформа: $PLATFORM"
        echo "💡 Поддерживаются: Linux, macOS, Windows"
        exit 1
        ;;
esac

echo ""
echo "📦 Создание дистрибутива..."

# Создаем директорию для сборки
mkdir -p build/midi_curves_v0.1.0

case $PLATFORM in
    "Linux")
        # Linux сборка - только VST3 плагин
        
        # Создаем правильную структуру VST3 плагина
        mkdir -p build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/x86_64-linux
        
        # Проверяем наличие VST3 плагина
        VST3_PLUGIN="target/release/deps/libvst_midi_curves.so"
        if [ -f "$VST3_PLUGIN" ]; then
            cp "$VST3_PLUGIN" build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/x86_64-linux/MidiCurves.so
            echo "✅ VST3 плагин скопирован"
        else
            echo "❌ VST3 плагин не найден: $VST3_PLUGIN"
            exit 1
        fi
        
        # Копируем Info.plist для Linux
        if [ -f "Info.plist.linux" ]; then
            cp Info.plist.linux build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/Info.plist
        else
            echo "❌ Info.plist.linux не найден"
            exit 1
        fi
        
        # Создаем директорию для ресурсов
        mkdir -p build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/Resources
        
        cp README.md build/midi_curves_v0.1.0/ 2>/dev/null || echo "📄 README.md не найден"
        cp BUILD_GUIDE.md build/midi_curves_v0.1.0/ 2>/dev/null || echo "📄 BUILD_GUIDE.md не найден"
        
        echo "🐧 Linux дистрибутив создан: build/midi_curves_v0.1.0/"
        echo "📝 VST3 плагин создан как MidiCurves.vst3/"
        ;;
    "Darwin")
        # macOS сборка - только VST3 плагин
        
        # Создаем правильную структуру VST3 плагина для macOS
        mkdir -p build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/x86_64-darwin
        mkdir -p build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/aarch64-darwin
        mkdir -p build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/Resources
        
        # Для упрощения копируем одинаковые файлы для обеих архитектур
        VST3_PLUGIN="target/release/deps/libvst_midi_curves.dylib"
        if [ -f "$VST3_PLUGIN" ]; then
            cp "$VST3_PLUGIN" build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/x86_64-darwin/MidiCurves
            cp "$VST3_PLUGIN" build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/aarch64-darwin/MidiCurves
            echo "✅ VST3 плагин скопирован для macOS"
        else
            echo "❌ VST3 плагин не найден для macOS"
            exit 1
        fi
        
        # Копируем Info.plist для macOS
        if [ -f "Info.plist.linux" ]; then
            cp Info.plist.linux build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/Info.plist
        else
            echo "❌ Info.plist.linux не найден"
            exit 1
        fi
        
        cp README.md build/midi_curves_v0.1.0/ 2>/dev/null || echo "📄 README.md не найден"
        cp BUILD_GUIDE.md build/midi_curves_v0.1.0/ 2>/dev/null || echo "📄 BUILD_GUIDE.md не найден"
        
        echo "🍎 macOS дистрибутив создан: build/midi_curves_v0.1.0/"
        ;;
esac

echo ""
echo "📋 Информация о сборке:"
echo "- VST3 плагин: $(ls -d build/midi_curves_v0.1.0/*.vst3 2>/dev/null || echo "не найдено")"

echo ""
echo "🎉 Сборка успешно завершена!"
echo "📁 Дистрибутив находится в: build/midi_curves_v0.1.0/"
echo ""
echo "📝 Для установки:"
echo "- Linux: скопировать MidiCurves.vst3 в ~/.vst3/"
echo "- macOS: скопировать MidiCurves.vst3 в ~/Library/Audio/Plug-Ins/VST3/"
echo "- Для Reaper: поместить в папку VST3, указанную в настройках"

## for test
rm -rf /home/sche/.vst3/MidiCurves.vst3
cp -rf /home/sche/programming/vst_midi_curves/build/midi_curves_v0.1.0/MidiCurves.vst3 /home/sche/.vst3/
flatpak run fm.reaper.Reaper
