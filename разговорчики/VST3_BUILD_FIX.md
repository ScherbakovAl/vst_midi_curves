# Решение проблемы: VST3 плагин не загружается в Reaper

## Исходная проблема
Пользователь сообщил, что:
- Standalone приложение работает отлично
- При сборке создавался VST3 плагин
- При попытке открыть файл в Reaper возникала неопределенная ошибка
- Вопросы: правильно ли собирается VST3 плагин, нет ли ошибок сборки, почему не открывается в Reaper

## Диагностика проблемы

### 1. Анализ исходного build.sh
Была обнаружена неправильная логика сборки:
- Скрипт искал файлы `midi_curves` (standalone приложение)
- Фактически создавался VST3 плагин как `libvst_midi_curves.so`
- Структура директорий не соответствовала стандарту VST3

### 2. Проверка созданных файлов
```bash
find target/release/ -name "*.so" -o -name "midi_curves" -o -name "*.dylib"
```
Результат показал, что создавался файл `libvst_midi_curves.so` в папке `deps/`, а не `midi_curves`

### 3. Проблемы структуры VST3
VST3 плагины требуют специальную структуру директорий:
```
PluginName.vst3/
└── Contents/
    ├── Info.plist
    ├── Resources/
    └── [Platform]/PluginName
```

## Решение

### 1. Исправление build.sh
- Изменена команда сборки с `--target x86_64-unknown-linux-gnu` на `--lib`
- Удалена логика сборки standalone приложения
- Добавлена проверка существования VST3 плагина
- Исправлена структура создания VST3 bundle

### 2. Создание Info.plist.linux
Создан файл метаданных VST3 плагина с правильной информацией:
- Идентификатор: `com.yourcompany.MidiCurves`
- Тип плагина: `aurg`
- Поддерживаемые форматы: аудио + MIDI

### 3. Исправление команд копирования
```bash
# Linux
cp target/release/deps/libvst_midi_curves.so build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/x86_64-linux/MidiCurves

# macOS
cp target/release/deps/libvst_midi_curves.dylib build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/x86_64-darwin/MidiCurves
cp target/release/deps/libvst_midi_curves.dylib build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/aarch64-darwin/MidiCurves
```

## Результат

### 1. Успешная сборка
```bash
./build.sh
```
Сборка прошла успешно с выводом:
- ✅ VST3 плагин скопирован
- 🐧 Linux дистрибутив создан: build/midi_curves_v0.1.0/
- 📝 VST3 плагин создан как MidiCurves.vst3/

### 2. Правильная структура
```
build/midi_curves_v0.1.0/
├── MidiCurves.vst3
│   └── Contents/
│       ├── Info.plist
│       ├── Resources
│       └── x86_64-linux/MidiCurves (4.5MB)
├── README.md
└── BUILD_GUIDE.md
```

### 3. Проверка файла
- Размер: 4.5MB (нормальный размер для Rust плагина с GUI)
- Права доступа: выполнимый файл (755)

## Основные исправления

### 1. Сборка библиотеки вместо приложения
**Было:**
```bash
cargo build --release --target x86_64-unknown-linux-gnu
```

**Стало:**
```bash
cargo build --release --lib
```

### 2. Правильные пути к VST3 плагину
**Было:**
```bash
cp target/release/midi_curves build/midi_curves_v0.1.0/
```

**Стало:**
```bash
cp target/release/deps/libvst_midi_curves.so build/midi_curves_v0.1.0/MidiCurves.vst3/Contents/x86_64-linux/MidiCurves
```

### 3. Создание Info.plist
**Добавлено:**
- Создание `Info.plist.linux` с метаданными плагина
- Копирование в правильную структуру VST3

## Причины проблемы

### 1. Неправильный скрипт сборки
Исходный `build.sh` был написан для standalone приложения, а не для VST3 плагина.

### 2. Отсутствие метаданных VST3
VST3 плагины требуют файл `Info.plist` с метаданными, который отсутствовал.

### 3. Неправильная структура файлов
Плагин создавался как обычная библиотека `.so`, а не как VST3 bundle.

## Дополнительные улучшения

### 1. Кроссплатформенная поддержка
- Linux: `x86_64-linux`
- macOS: `x86_64-darwin` и `aarch64-darwin`

### 2. Проверки ошибок
- Проверка существования файлов перед копированием
- Остановка скрипта при ошибках

### 3. Информативные сообщения
- Подробный вывод о процессе сборки
- Инструкции по установке

## Заключение

**Основная причина проблемы:** Неправильная структура сборки VST3 плагина.

**Решение:** Исправлен скрипт сборки для создания правильной VST3 структуры с метаданными.

**Результат:** VST3 плагин теперь корректно создается и готов к установке в Reaper.

**Следующие шаги:**
1. Скопировать `MidiCurves.vst3` в `~/.vst3/`
2. Перезапустить Reaper
3. Плагин должен появиться в списке VST3