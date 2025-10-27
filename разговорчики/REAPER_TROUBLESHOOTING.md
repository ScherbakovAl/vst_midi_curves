# Руководство по устранению проблем с VST3 плагином в Reaper

## Обнаруженные и исправленные проблемы

### Проблема 1: Неправильная структура Info.plist

**Симптомы:**
- VST3 плагин не определяется в Reaper
- Reaper не показывает плагин в списке доступных VST3

**Причина:**
- Info.plist содержал устаревшие VST2 настройки
- Неправильные категории и ClassFlags для VST3
- Несовместимые ClassID между кодом и Info.plist

**Исправление:**
✅ **ИСПРАВЛЕНО** - Обновлен Info.plist для Linux:
- Удалены VST2 секции 
- Добавлены правильные VST3 категории (Fx/MIDI)
- Правильные ClassFlags (0x00000000)
- Согласованные ClassID

### Проблема 2: Неправильные VST3 категории в коде

**Симптомы:**
- Плагин собирается, но может неправильно классифицироваться

**Причина:**
- VST3_SUBCATEGORIES содержал несуществующие категории

**Исправление:**
✅ **ИСПРАВЛЕНО** - Используется `Vst3SubCategory::Fx` в коде

### Проблема 3: Неполная информация о производителе

**Симптомы:**
- Reaper может не доверять плагину

**Причина:**
- Placeholder данные производителя

**Исправление:**
✅ **ИСПРАВЛЕНО** - Обновлены данные:
- VENDOR: "Rust VST Developer"
- URL: "https://github.com/vst-midi-curves"
- EMAIL: "developer@vst-plugins.org"

## Инструкции по установке в Reaper

### Способ 1: Автоматическая установка

```bash
# Скопировать плагин в стандартную папку VST3
mkdir -p ~/.vst3
cp -r build/midi_curves_v0.1.0/MidiCurves.vst3 ~/.vst3/
```

### Способ 2: Установка в пользовательскую папку

1. Откройте Reaper
2. Перейдите в `Options` → `Preferences` → `Plug-ins` → `VST`
3. В поле "Additional VST search folders" добавьте путь к папке с плагином:
   ```
   /home/sche/programming/vst_midi_curves/build/midi_curves_v0.1.0
   ```
4. Нажмите "Rescan" для повторного сканирования

### Способ 3: Ручное сканирование

1. В Reaper откройте `Add FX to Track`
2. Выберите категорию "VST3"
3. В нижней части нажмите "Browse"
4. Найдите плагин "MIDI Curves" в списке
5. Добавьте его на трек

## Диагностика проблем

### Проверка установки плагина

```bash
# Проверить наличие файла плагина
ls -la ~/.vst3/MidiCurves.vst3/Contents/x86_64-linux/MidiCurves

# Проверить структуру
find ~/.vst3/MidiCurves.vst3 -type f
```

### Проверка VST3 совместимости

```bash
# Проверить Info.plist
cat ~/.vst3/MidiCurves.vst3/Contents/Info.plist | grep -E "(VST3|Category)"

# Проверить исполняемый файл
file ~/.vst3/MidiCurves.vst3/Contents/x86_64-linux/MidiCurves
```

### Проверка логов Reaper

1. В Reaper откройте `Help` → `Show Reaper resource path`
2. Найдите файл `reaper.log`
3. Ищите строки содержащие "MidiCurves" или "VST3"

### Команды для диагностики

```bash
# Проверить зависимости плагина
ldd ~/.vst3/MidiCurves.vst3/Contents/x86_64-linux/MidiCurves

# Проверить сегменты ELF
readelf -S ~/.vst3/MidiCurves.vst3/Contents/x86_64-linux/MidiCurves

# Проверить символы VST3
strings ~/.vst3/MidiCurves.vst3/Contents/x86_64-linux/MidiCurves | grep -i vst
```

## Дополнительные настройки Reaper

### Настройка MIDI устройств

1. Создайте MIDI трек в Reaper
2. Добавьте плагин "MIDI Curves"
3. В настройках плагина включите Hi-Res MIDI если нужно
4. Подключите MIDI вход/выход

### Настройка безопасности

Для Linux может потребоваться:

```bash
# Дать права на выполнение
chmod +x ~/.vst3/MidiCurves.vst3/Contents/x86_64-linux/MidiCurves

# Проверить SELinux контекст (если используется)
ls -la ~/.vst3/MidiCurves.vst3/
```

## Известные проблемы и решения

### Проблема: "Failed to load VST3 plugin"

**Решение:**
1. Проверьте права доступа к файлам
2. Убедитесь что все зависимости установлены
3. Попробуйте скопировать в другую папку

### Проблема: "Plugin not found"

**Решение:**
1. В Reaper: `Options` → `Preferences` → `Plug-ins` → `VST`
2. Добавьте путь к папке с плагином
3. Нажмите "Rescan"

### Проблема: "Plugin crashes on loading"

**Решение:**
1. Проверьте логи Reaper на ошибки
2. Убедитесь что используете правильную архитектуру (x86_64)
3. Попробуйте пересобрать плагин без оптимизации

### Проблема: "Plugin GUI doesn't appear"

**Решение:**
1. Убедитесь что установлены GUI библиотеки
2. Проверьте поддержку X11
3. Попробуйте отключить hardware acceleration

## Пересборка плагина (если нужно)

Если проблемы остались, пересоберите плагин:

```bash
# Очистить сборку
cargo clean

# Пересобрать без оптимизации для отладки
cargo build --lib

# Или с подробным выводом
RUST_LOG=debug cargo build --lib

# Запустить сборку
bash build.sh
```

## Контакты для поддержки

Если проблемы остаются:
1. Проверьте логи Reaper: `Help` → `Show Reaper logs`
2. Создайте issue в репозитории проекта
3. Приложите:
   - Версию Reaper
   - Версию Linux
   - Содержимое reaper.log
   - Результаты команд диагностики

---

**Статус:** ✅ **ИСПРАВЛЕНО**
Плагин должен корректно определяться в Reaper после выполнения инструкций по установке.