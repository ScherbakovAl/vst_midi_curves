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
        cargo build --release --target x86_64-unknown-linux-gnu
        echo "✅ Сборка для Linux завершена!"
        ;;
    "Darwin")
        echo "🍎 Сборка для macOS..."
        
        # Проверяем архитектуру
        ARCH=$(uname -m)
        if [ "$ARCH" = "arm64" ]; then
            echo "🍎 Обнаружен Apple Silicon (ARM64)"
            # Создаем Universal Binary для macOS
            rustup target add x86_64-apple-darwin aarch64-apple-darwin
            cargo build --release --target x86_64-apple-darwin
            cargo build --release --target aarch64-apple-darwin
            echo "🔗 Создание Universal Binary..."
            lipo -create target/release/midi_curves -target x86_64-apple-darwin \
                 target/release/midi_curves -target aarch64-apple-darwin \
                 -output target/release/midi_curves_universal
            echo "✅ Universal Binary создан: target/release/midi_curves_universal"
        else
            echo "🍎 Обнаружен Intel Mac"
            rustup target add x86_64-apple-darwin
            cargo build --release --target x86_64-apple-darwin
        fi
        
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
        # Linux сборка
        cp target/release/midi_curves build/midi_curves_v0.1.0/
        cp target/release/libvst_midi_curves.so build/midi_curves_v0.1.0/
        cp README.md build/midi_curves_v0.1.0/
        cp BUILD_GUIDE.md build/midi_curves_v0.1.0/
        
        echo "🐧 Linux дистрибутив создан: build/midi_curves_v0.1.0/"
        ;;
    "Darwin")
        # macOS сборка
        if [ -f "target/release/midi_curves_universal" ]; then
            cp target/release/midi_curves_universal build/midi_curves_v0.1.0/midi_curves
            chmod +x build/midi_curves_v0.1.0/midi_curves
        else
            cp target/release/midi_curves build/midi_curves_v0.1.0/
            chmod +x build/midi_curves_v0.1.0/midi_curves
        fi
        
        # VST3 плагин для macOS
        lipo -create target/release/libvst_midi_curves.dylib \
             -target x86_64-apple-darwin \
             target/release/libvst_midi_curves.dylib \
             -target aarch64-apple-darwin \
             -output build/midi_curves_v0.1.0/libvst_midi_curves.dylib
        
        cp README.md build/midi_curves_v0.1.0/
        cp BUILD_GUIDE.md build/midi_curves_v0.1.0/
        
        echo "🍎 macOS дистрибутив создан: build/midi_curves_v0.1.0/"
        ;;
esac

echo ""
echo "📋 Информация о сборке:"
echo "- Standalone приложение: $(ls -lh build/midi_curves_v0.1.0/midi_curves 2>/dev/null || echo "не найдено")"
echo "- VST3 плагин: $(ls -lh build/midi_curves_v0.1.0/libvst_midi_curves.* 2>/dev/null || echo "не найдено")"

echo ""
echo "🎉 Сборка успешно завершена!"
echo "📁 Дистрибутив находится в: build/midi_curves_v0.1.0/"
echo ""
echo "📝 Для установки:"
echo "- Linux: скопировать libvst_midi_curves.so в ~/.vst3/"
echo "- macOS: скопировать libvst_midi_curves.dylib в ~/Library/Audio/Plug-Ins/VST3/"
echo "- Standalone: запустить ./midi_curves"