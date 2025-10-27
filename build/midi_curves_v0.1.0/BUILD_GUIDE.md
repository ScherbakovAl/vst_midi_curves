# Сборка VST3 MIDI Curves

## 🎯 Обзор проекта

Ваш проект теперь готов для сборки как VST3 плагин и standalone приложение для всех основных платформ:

- **🪟 Windows** (x86_64)
- **🍎 macOS** (Intel + Apple Silicon Universal Binary)
- **🐧 Linux** (x86_64)

## 🏗️ Быстрый старт

### Предварительные требования

1. **Rust** (версия 1.70+)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup update
   ```

2. **Системные зависимости**
   - **Windows**: `cross` для кросс-компиляции
   - **macOS**: XCode Command Line Tools
   - **Linux**: GTK4 development headers

### Команды сборки

#### Автоматическая сборка (рекомендуется)
```bash
# Сборка для текущей платформы
./build.sh current

# Сборка для конкретной платформы
./build.sh windows
./build.sh macos
./build.sh linux

# Сборка для всех платформ
./build.sh all

# Очистка и пересборка
./build.sh current --clean

# Справка по командам
./build.sh help
```

#### Ручная сборка
```bash
# VST3 плагин
cargo build --release --bin midi_curves_vst3

# Standalone приложение
cargo build --release --bin midi_curves
```

## 📋 Детальные инструкции по платформам

### 🪟 Windows

#### Установка зависимостей
```bash
# Установка cross для кросс-компиляции
cargo install cross

# Проверка установки
cross --version
```

#### Сборка
```bash
# Использование скрипта
./build_windows.sh

# Или ручная сборка
cross build --release --target x86_64-pc-windows-msvc --bin midi_curves_vst3
cross build --release --target x86_64-pc-windows-msvc --bin midi_curves
```

#### Результат сборки
```
📂 target/x86_64-pc-windows-msvc/release/
├── midi_curves_vst3.dll        # VST3 плагин
└── midi_curves.exe             # Standalone приложение
```

#### Установка VST3
1. Скопируйте `midi_curves_vst3.dll` в папку плагинов
2. Пути установки VST3 для Windows:
   - `C:\Program Files\Common Files\VST3\`
   - `C:\Users\<username>\Documents\VST3\`

#### Установка Standalone
1. Скопируйте `midi_curves.exe` в желаемую папку
2. Запустите приложение

---

### 🍎 macOS

#### Установка зависимостей
```bash
# XCode Command Line Tools
xcode-select --install

# Проверка установки
xcode-select -p
```

#### Сборка
```bash
# Использование скрипта (рекомендуется)
./build_macos.sh

# Скрипт создаст Universal Binary (Intel + Apple Silicon)
```

#### Результат сборки
```
📂 target/universal/
├── MidiCurves.vst3/            # VST3 плагин (Universal)
│   ├── Contents/
│   │   ├── Info.plist
│   │   ├── PkgInfo
│   │   └── MacOS/
│   │       └── MidiCurves      # Universal Binary
└── midi_curves                 # Standalone приложение (Universal)
```

#### Установка VST3
```bash
# Для всех пользователей (требует sudo)
sudo cp -r "MidiCurves.vst3" /Library/Audio/Plug-Ins/VST3/

# Для текущего пользователя
mkdir -p ~/Library/Audio/Plug-Ins/VST3/
cp -r "MidiCurves.vst3" ~/Library/Audio/Plug-Ins/VST3/
```

#### Установка Standalone
```bash
# Перемещение в Applications
cp "midi_curves" /Applications/

# Установка прав (если нужно)
chmod +x /Applications/midi_curves
```

#### Безопасность macOS
При первом запуске:
1. Система может показать предупреждение "невозможно проверить разработчика"
2. Откройте `System Preferences` > `Security & Privacy`
3. Нажмите `Allow Anyway` для приложения

---

### 🐧 Linux

#### Установка зависимостей

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install pkg-config libgtk-4-dev build-essential curl wget git
```

**Fedora:**
```bash
sudo dnf install pkg-config gtk4-devel gcc gcc-c++ curl wget git
```

**Arch Linux:**
```bash
sudo pacman -S pkg-config gtk4 base-devel curl wget git
```

#### Сборка
```bash
# Использование скрипта
./build_linux.sh

# Или ручная сборка
cargo build --release --bin midi_curves_vst3
cargo build --release --bin midi_curves
```

#### Результат сборки
```
📂 target/release/
├── midi_curves_vst3.so         # VST3 плагин
└── midi_curves                 # Standalone приложение
```

#### Структура VST3 для Linux
```bash
mkdir -p vst3/x86_64-linux/
cp midi_curves_vst3.so vst3/x86_64-linux/MidiCurves.vst3
```

#### Установка VST3
```bash
# Для текущего пользователя
mkdir -p ~/.vst3/
cp -r "MidiCurves.vst3" ~/.vst3/

# Для всех пользователей (требует sudo)
sudo cp -r "MidiCurves.vst3" /usr/lib/vst3/
```

#### Запуск Standalone
```bash
# Прямой запуск
./target/release/midi_curves

# С установленным desktop файлом
# Найдите "MIDI Curves" в меню приложений
```

## 🔧 Устранение проблем

### Общие проблемы

#### 1. Ошибки компиляции
```bash
# Очистка кэша
cargo clean

# Обновление Rust
rustup update

# Пересборка
cargo build --release
```

#### 2. Недостающие зависимости
```bash
# Проверка системных зависимостей
# Windows: Установите Visual Studio Build Tools
# macOS: xcode-select --install
# Linux: Проверьте GTK4 и pkg-config
```

#### 3. Ошибки линковки на Linux
```bash
# Установка дополнительных библиотек
sudo apt install libssl-dev libasound2-dev libudev-dev libcairo2-dev libgdk-pixbuf2.0-dev
```

### Специфичные проблемы

#### Windows: "cross not found"
```bash
cargo install cross
```

#### macOS: "lipo not found"
```bash
xcode-select --install
```

#### Linux: "pkg-config not found"
```bash
# Ubuntu/Debian
sudo apt install pkg-config

# Fedora
sudo dnf install pkg-config

# Arch
sudo pacman -S pkg-config
```

## 📊 Тестирование

### Тестирование VST3 плагина

1. **Компиляция**: Проверьте, что компиляция проходит без ошибок
2. **Загрузка в DAW**: Протестируйте в популярных DAW:
   - Reaper (кроссплатформенный)
   - Ableton Live (Windows/macOS)
   - Logic Pro (только macOS)
   - FL Studio (только Windows)

3. **Функциональность**: Проверьте:
   - Загрузка и сохранение пресетов
   - Обработка MIDI velocity
   - GUI отзывчивость
   - MIDI вход/выход

### Тестирование Standalone

1. **Запуск**: Проверьте запуск приложения
2. **MIDI порты**: Проверьте обнаружение MIDI устройств
3. **Обработка**: Протестируйте обработку MIDI событий
4. **GUI**: Проверьте интерактивность графика

## 🚀 Распространение

### Подготовка релизов

```bash
# Создание релизной папки
mkdir -p releases/v0.1.0
cd releases

# Скопируйте все платформенные билды
cp -r ../../target/x86_64-pc-windows-msvc/release/* windows/
cp -r ../../target/universal/* macos/
cp -r ../../target/release/* linux/

# Создайте архивы
zip -r midi-curves-windows.zip windows/
zip -r midi-curves-macos.zip macos/
tar -czf midi-curves-linux.tar.gz linux/
```

### Рекомендации по распространению

1. **Подпись кода** (для macOS и Windows)
2. **Версионирование**: Используйте семантическое версионирование
3. **Changelog**: Ведите журнал изменений
4. **Документация**: Предоставьте user manual

## 📚 Дополнительные ресурсы

### Документация
- [NIH-plug Book](https://nih-plug.robbert.vdh.org/)
- [VST3 SDK](https://steinbergmedia.github.io/vst3_doc/)
- [egui Documentation](https://docs.rs/egui/latest/egui/)

### Примеры проектов
- [NIH-plug Examples](https://github.com/robbert-vdh/nih-plug/tree/master/plugins)
- [Rust Audio Plugins](https://github.com/rust-audio/)

### Сообщество
- [Rust Audio Discord](https://discord.gg/rust-audio)
- [NIН-plug Discussions](https://github.com/robbert-vdh/nih-plug/discussions)

---

**Версия документа:** 1.0  
**Дата:** 2025-10-25  
**Статус:** Готов к использованию