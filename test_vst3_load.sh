#!/bin/bash
# Диагностический скрипт для проверки VST3 плагина

echo "🔍 Диагностика VST3 плагина MidiCurves"
echo "========================================"
echo ""

VST3_PATH="build/midi_curves_v0.1.0/MidiCurves.vst3"

# 1. Проверка структуры
echo "1️⃣ Проверка структуры VST3 бандла:"
if [ -d "$VST3_PATH" ]; then
    echo "✅ VST3 бандл существует"
    tree -L 3 "$VST3_PATH"
else
    echo "❌ VST3 бандл не найден!"
    exit 1
fi
echo ""

# 2. Проверка бинарного файла
echo "2️⃣ Проверка бинарного файла:"
BINARY="$VST3_PATH/Contents/x86_64-linux/MidiCurves.so"
if [ -f "$BINARY" ]; then
    echo "✅ Бинарный файл существует"
    echo "   Тип: $(file $BINARY | cut -d: -f2)"
    echo "   Размер: $(du -h $BINARY | cut -f1)"
    echo "   Права: $(ls -l $BINARY | awk '{print $1}')"
else
    echo "❌ Бинарный файл не найден!"
    exit 1
fi
echo ""

# 3. Проверка экспортируемых символов
echo "3️⃣ Проверка экспортируемых символов VST3:"
REQUIRED_SYMBOLS=("GetPluginFactory" "ModuleEntry" "ModuleExit")
MISSING=0

for symbol in "${REQUIRED_SYMBOLS[@]}"; do
    if nm -D "$BINARY" | grep -q " T $symbol"; then
        echo "✅ $symbol"
    else
        echo "❌ $symbol - ОТСУТСТВУЕТ!"
        MISSING=1
    fi
done

if [ $MISSING -eq 1 ]; then
    echo ""
    echo "⚠️  ВНИМАНИЕ: Отсутствуют обязательные символы!"
    exit 1
fi
echo ""

# 4. Проверка Info.plist
echo "4️⃣ Проверка Info.plist:"
PLIST="$VST3_PATH/Contents/Info.plist"
if [ -f "$PLIST" ]; then
    echo "✅ Info.plist существует"
    echo ""
    echo "Ключевые параметры:"
    echo "  Имя: $(grep -A1 '<key>CFBundleName</key>' $PLIST | grep string | sed 's/.*<string>\(.*\)<\/string>.*/\1/')"
    echo "  ID: $(grep -A1 '<key>CFBundleIdentifier</key>' $PLIST | grep string | sed 's/.*<string>\(.*\)<\/string>.*/\1/')"
    echo "  Версия: $(grep -A1 '<key>CFBundleVersion</key>' $PLIST | grep string | sed 's/.*<string>\(.*\)<\/string>.*/\1/')"
    echo "  Категория: $(grep -A1 '<key>Category</key>' $PLIST | grep string | sed 's/.*<string>\(.*\)<\/string>.*/\1/' | head -1)"
    echo "  SubCategories: $(grep -A1 '<key>SubCategories</key>' $PLIST | grep string | sed 's/.*<string>\(.*\)<\/string>.*/\1/')"
else
    echo "❌ Info.plist не найден!"
    exit 1
fi
echo ""

# 5. Проверка зависимостей
echo "5️⃣ Проверка зависимостей библиотек:"
echo "Используется ldd для проверки..."
MISSING_LIBS=$(ldd "$BINARY" | grep "not found" | wc -l)
if [ $MISSING_LIBS -eq 0 ]; then
    echo "✅ Все библиотеки найдены"
    echo ""
    echo "Основные зависимости:"
    ldd "$BINARY" | grep -E "(libGL|libX11|libasound|libgcc|libc\.so)" | sed 's/^/  /'
else
    echo "❌ Отсутствуют библиотеки:"
    ldd "$BINARY" | grep "not found" | sed 's/^/  /'
    exit 1
fi
echo ""

# 6. Проверка на наличие Reaper
echo "6️⃣ Проверка Reaper:"
if command -v reaper &> /dev/null; then
    echo "✅ Reaper установлен: $(which reaper)"
    REAPER_VERSION=$(reaper -v 2>&1 || echo "Не удалось определить версию")
    echo "   Версия: $REAPER_VERSION"
else
    echo "⚠️  Reaper не найден в PATH"
fi
echo ""

# 7. Проверка VST3 путей Reaper
echo "7️⃣ Проверка VST3 директорий:"
VST3_DIRS=(
    "$HOME/.vst3"
    "/usr/lib/vst3"
    "/usr/local/lib/vst3"
)

for dir in "${VST3_DIRS[@]}"; do
    if [ -d "$dir" ]; then
        echo "✅ $dir (существует)"
        if [ -d "$dir/MidiCurves.vst3" ]; then
            echo "   ⚠️  MidiCurves.vst3 уже установлен здесь!"
        fi
    else
        echo "➖ $dir (не существует)"
    fi
done
echo ""

# 8. Рекомендации
echo "📋 Рекомендации:"
echo ""
echo "1. Установите плагин:"
echo "   mkdir -p ~/.vst3"
echo "   cp -r $VST3_PATH ~/.vst3/"
echo ""
echo "2. Перезапустите Reaper"
echo ""
echo "3. В Reaper:"
echo "   - Options -> Preferences -> Plug-ins -> VST"
echo "   - Нажмите 'Re-scan' для повторного сканирования"
echo "   - Проверьте 'Clear cache/re-scan' если плагин не появляется"
echo ""
echo "4. Проверьте лог Reaper в:"
echo "   ~/.config/REAPER/reaper.ini (настройки)"
echo "   ~/.config/REAPER/reaper-vstplugins64.ini (кэш плагинов)"
echo ""

# 9. Попытка найти логи ошибок Reaper
echo "9️⃣ Поиск логов Reaper:"
REAPER_LOG="$HOME/.config/REAPER/reaper-vstplugins64.ini"
if [ -f "$REAPER_LOG" ]; then
    echo "✅ Найден кэш плагинов: $REAPER_LOG"
    if grep -q "MidiCurves" "$REAPER_LOG"; then
        echo ""
        echo "Информация о MidiCurves в кэше:"
        grep -A5 "MidiCurves" "$REAPER_LOG" | sed 's/^/  /'
    else
        echo "   ℹ️  MidiCurves не найден в кэше (плагин ещё не сканировался)"
    fi
else
    echo "➖ Кэш плагинов не найден"
fi
echo ""

echo "✅ Диагностика завершена!"
echo ""
echo "💡 Если плагин всё ещё не загружается в Reaper:"
echo "   1. Проверьте, нет ли конфликтов с другими плагинами"
echo "   2. Попробуйте запустить Reaper из терминала для просмотра ошибок:"
echo "      reaper 2>&1 | tee reaper_log.txt"
echo "   3. Проверьте, что Reaper имеет права на чтение VST3 папки"