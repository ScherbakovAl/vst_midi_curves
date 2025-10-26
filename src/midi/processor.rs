//! Обработка и фильтрация MIDI событий
//! 
//! Этот модуль отвечает за:
//! - Парсинг и валидацию MIDI данных
//! - Фильтрацию MIDI событий по каналам и типам
//! - Применение velocity кривых
//! - Маршрутизацию MIDI событий

use std::collections::HashMap;
use std::time::Instant;

// События MIDI для внутренней обработки
#[derive(Debug, Clone, PartialEq)]
pub struct RawMidiEvent {
    pub data: Vec<u8>,
    pub timestamp: u64,
    pub channel: Option<u8>,
    pub message_type: MidiMessageType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MidiMessageType {
    NoteOn,
    NoteOff,
    ControlChange,
    ProgramChange,
    PitchBend,
    ChannelPressure,
    PolyphonicKeyPressure,
    SystemExclusive,
    SystemCommon,
    SystemRealtime,
    Unknown,
}

// Конфигурация обработчика
#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    pub enabled_channels: Vec<u8>,  // Если пусто, все каналы разрешены
    pub disabled_channels: Vec<u8>, // Если не пусто, эти каналы заблокированы
    pub enabled_messages: Vec<MidiMessageType>, // Если пусто, все сообщения разрешены
    pub disabled_messages: Vec<MidiMessageType>, // Если не пусто, эти сообщения заблокированы
    pub velocity_curve_enabled: bool,
    pub note_stacking: bool, // Объединение NoteOn/NoteOff для одного note
    pub channel_offset: i8,  // Смещение канала при обработке (0 = без изменений)
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            enabled_channels: vec![],
            disabled_channels: vec![],
            enabled_messages: vec![],
            disabled_messages: vec![],
            velocity_curve_enabled: true,
            note_stacking: false,
            channel_offset: 0,
        }
    }
}

// Структура для отслеживания активных нот
#[derive(Debug, Clone)]
struct ActiveNote {
    note: u8,
    velocity: u8,
    start_time: Instant,
    channel: u8,
}

// Основной процессор MIDI событий
pub struct MidiEventProcessor {
    config: ProcessorConfig,
    active_notes: HashMap<u8, ActiveNote>, // note -> info (для note_stacking)
    stats: ProcessorStats,
}

#[derive(Debug, Clone, Default)]
struct ProcessorStats {
    messages_processed: u64,
    messages_filtered: u64,
    notes_started: u64,
    notes_stopped: u64,
    control_changes: u64,
    errors: u64,
}

impl MidiEventProcessor {
    pub fn new() -> Self {
        Self {
            config: ProcessorConfig::default(),
            active_notes: HashMap::new(),
            stats: ProcessorStats::default(),
        }
    }
    
    pub fn new_with_config(config: ProcessorConfig) -> Self {
        Self {
            config,
            active_notes: HashMap::new(),
            stats: ProcessorStats::default(),
        }
    }
    
    /// Обработка входящего MIDI сообщения
    pub fn process_midi_data(&mut self, data: &[u8], _timestamp: u64) -> Option<Vec<u8>> {
        self.stats.messages_processed += 1;
        
        // Парсинг MIDI данных
        let mut event = match self.parse_midi_data(data) {
            Ok(event) => event,
            Err(e) => {
                eprintln!("Ошибка парсинга MIDI: {}", e);
                self.stats.errors += 1;
                return None;
            }
        };
        
        // Фильтрация по каналу
        if !self.is_channel_allowed(event.channel) {
            self.stats.messages_filtered += 1;
            return None;
        }
        
        // Фильтрация по типу сообщения
        if !self.is_message_type_allowed(&event.message_type) {
            self.stats.messages_filtered += 1;
            return None;
        }
        
        // Применение специфичной обработки для разных типов сообщений
        match event.message_type {
            MidiMessageType::NoteOn => {
                self.stats.notes_started += 1;
                if let Err(e) = self.handle_note_on(&mut event) {
                    eprintln!("Ошибка обработки NoteOn: {}", e);
                }
            }
            MidiMessageType::NoteOff => {
                self.stats.notes_stopped += 1;
                if let Err(e) = self.handle_note_off(&mut event) {
                    eprintln!("Ошибка обработки NoteOff: {}", e);
                }
            }
            MidiMessageType::ControlChange => {
                self.stats.control_changes += 1;
            }
            _ => {}
        }
        
        // Применение смещения канала
        if let Some(channel) = event.channel_mut() {
            let new_channel = ((*channel as i16 + self.config.channel_offset as i16) % 16) as u8;
            *channel = new_channel;
        }
        
        // Преобразование обратно в MIDI данные
        Some(event.data)
    }
    
    /// Парсинг MIDI данных в структуру события
    fn parse_midi_data(&self, data: &[u8]) -> Result<RawMidiEvent, &'static str> {
        if data.is_empty() {
            return Err("Пустые MIDI данные");
        }
        
        let status = data[0];
        let (message_type, channel) = match status & 0xF0 {
            0x80 => (MidiMessageType::NoteOff, Some(status & 0x0F)),
            0x90 => (MidiMessageType::NoteOn, Some(status & 0x0F)),
            0xA0 => (MidiMessageType::PolyphonicKeyPressure, Some(status & 0x0F)),
            0xB0 => (MidiMessageType::ControlChange, Some(status & 0x0F)),
            0xC0 => (MidiMessageType::ProgramChange, Some(status & 0x0F)),
            0xD0 => (MidiMessageType::ChannelPressure, Some(status & 0x0F)),
            0xE0 => (MidiMessageType::PitchBend, Some(status & 0x0F)),
            0xF0 => match status {
                0xF0..=0xF7 => (MidiMessageType::SystemExclusive, None),
                0xF8..=0xFF => (MidiMessageType::SystemRealtime, None),
                _ => (MidiMessageType::Unknown, None),
            },
            _ => (MidiMessageType::Unknown, None),
        };
        
        Ok(RawMidiEvent {
            data: data.to_vec(),
            timestamp: 0, // Будет установлен внешним кодом
            channel,
            message_type,
        })
    }
    
    /// Проверка разрешенности канала
    fn is_channel_allowed(&self, channel: Option<u8>) -> bool {
        if let Some(ch) = channel {
            // Проверка disabled_channels
            if self.config.disabled_channels.contains(&ch) {
                return false;
            }
            
            // Проверка enabled_channels (если есть)
            if !self.config.enabled_channels.is_empty() {
                return self.config.enabled_channels.contains(&ch);
            }
        }
        
        true
    }
    
    /// Проверка разрешенности типа сообщения
    fn is_message_type_allowed(&self, message_type: &MidiMessageType) -> bool {
        // Проверка disabled_messages
        if self.config.disabled_messages.contains(message_type) {
            return false;
        }
        
        // Проверка enabled_messages (если есть)
        if !self.config.enabled_messages.is_empty() {
            return self.config.enabled_messages.contains(message_type);
        }
        
        true
    }
    
    /// Обработка NoteOn события
    fn handle_note_on(&self, event: &mut RawMidiEvent) -> Result<(), &'static str> {
        if event.data.len() < 3 {
            return Err("Недостаточно данных для NoteOn");
        }
        
        let velocity = event.data[2];
        
        // Если velocity = 0, это NoteOff в маскировке
        if velocity == 0 {
            event.message_type = MidiMessageType::NoteOff;
            return Ok(());
        }
        
        // Здесь можно применить velocity кривую
        // Но это будет сделано в основном MIDI менеджере
        
        Ok(())
    }
    
    /// Обработка NoteOff события
    fn handle_note_off(&self, event: &mut RawMidiEvent) -> Result<(), &'static str> {
        if event.data.len() < 3 {
            return Err("Недостаточно данных для NoteOff");
        }
        
        // Если включен note_stacking, можно проверить соответствие с NoteOn
        if self.config.note_stacking {
            let _note = event.data[1];
            // Логика отслеживания стека нот
            // ...
        }
        
        Ok(())
    }
    
    /// Установка конфигурации обработчика
    pub fn set_config(&mut self, config: ProcessorConfig) {
        self.config = config;
        self.clear_active_notes();
    }
    
    /// Получение текущей конфигурации
    pub fn get_config(&self) -> &ProcessorConfig {
        &self.config
    }
    
    /// Очистка активных нот (для note_stacking)
    pub fn clear_active_notes(&mut self) {
        self.active_notes.clear();
    }
    
    /// Получение статистики обработки
    pub fn get_stats(&self) -> &ProcessorStats {
        &self.stats
    }
    
    /// Сброс статистики
    pub fn reset_stats(&mut self) {
        self.stats = ProcessorStats::default();
    }
    
    /// Проверка наличия активных нот
    pub fn has_active_notes(&self) -> bool {
        !self.active_notes.is_empty()
    }
    
    /// Получение списка активных нот
    pub fn get_active_notes(&self) -> Vec<&ActiveNote> {
        self.active_notes.values().collect()
    }
    
    /// Принудительное закрытие всех активных нот
    pub fn force_all_notes_off(&mut self) -> Vec<Vec<u8>> {
        let mut note_off_messages = Vec::new();
        
        for active_note in self.active_notes.values() {
            let note_off = vec![
                0x80 | (active_note.channel & 0x0F),
                active_note.note,
                64, // Velocity для NoteOff
            ];
            note_off_messages.push(note_off);
        }
        
        self.active_notes.clear();
        note_off_messages
    }
}

// Утилиты для работы с MIDI процессором
pub mod utils {
    use super::*;
    
    /// Создание конфигурации для фильтрации по каналам
    pub fn config_for_channels(channels: Vec<u8>) -> ProcessorConfig {
        ProcessorConfig {
            enabled_channels: channels,
            disabled_channels: vec![],
            enabled_messages: vec![],
            disabled_messages: vec![],
            velocity_curve_enabled: true,
            note_stacking: false,
            channel_offset: 0,
        }
    }
    
    /// Создание конфигурации для обработки только NoteOn/NoteOff
    pub fn config_for_notes_only() -> ProcessorConfig {
        ProcessorConfig {
            enabled_channels: vec![],
            disabled_channels: vec![],
            enabled_messages: vec![
                MidiMessageType::NoteOn,
                MidiMessageType::NoteOff,
            ],
            disabled_messages: vec![],
            velocity_curve_enabled: true,
            note_stacking: false,
            channel_offset: 0,
        }
    }
    
    /// Создание конфигурации для обработки только CC сообщений
    pub fn config_for_control_change_only() -> ProcessorConfig {
        ProcessorConfig {
            enabled_channels: vec![],
            disabled_channels: vec![],
            enabled_messages: vec![MidiMessageType::ControlChange],
            disabled_messages: vec![],
            velocity_curve_enabled: false,
            note_stacking: false,
            channel_offset: 0,
        }
    }
    
    /// Создание конфигурации с отключенным velocity curve
    pub fn config_without_velocity_curve() -> ProcessorConfig {
        ProcessorConfig {
            enabled_channels: vec![],
            disabled_channels: vec![],
            enabled_messages: vec![],
            disabled_messages: vec![],
            velocity_curve_enabled: false,
            note_stacking: false,
            channel_offset: 0,
        }
    }
}

// Дополнительные методы для RawMidiEvent
impl RawMidiEvent {
    /// Получение изменяемой ссылки на канал
    fn channel_mut(&mut self) -> Option<&mut u8> {
        match self.message_type {
            MidiMessageType::Unknown => None,
            MidiMessageType::SystemExclusive | MidiMessageType::SystemCommon | MidiMessageType::SystemRealtime => None,
            _ => self.channel.as_mut(),
        }
    }
    
    /// Проверка, является ли сообщение голосовым (channel voice)
    fn is_voice_message(&self) -> bool {
        match self.message_type {
            MidiMessageType::SystemExclusive | MidiMessageType::SystemCommon | MidiMessageType::SystemRealtime => false,
            MidiMessageType::Unknown => false,
            _ => true,
        }
    }
    
    /// Получение статуса байта
    fn get_status_byte(&self) -> Option<u8> {
        self.data.first().copied()
    }
    
    /// Проверка валидности MIDI данных
    fn is_valid(&self) -> bool {
        if self.data.is_empty() {
            return false;
        }
        
        let status = self.data[0];
        
        // Системные сообщения
        if status >= 0xF8 {
            return matches!(
                self.message_type,
                MidiMessageType::SystemRealtime
            );
        }
        
        if status >= 0xF0 {
            return matches!(
                self.message_type,
                MidiMessageType::SystemExclusive | MidiMessageType::SystemCommon
            );
        }
        
        // Голосовые сообщения должны иметь канал
        if !self.is_voice_message() {
            return false;
        }
        
        if let Some(channel) = self.channel {
            if (status & 0x0F) != (channel & 0x0F) {
                return false;
            }
        }
        
        match self.message_type {
            MidiMessageType::NoteOn | MidiMessageType::NoteOff | MidiMessageType::ControlChange | MidiMessageType::PolyphonicKeyPressure | MidiMessageType::PitchBend => {
                self.data.len() == 3
            }
            MidiMessageType::ProgramChange | MidiMessageType::ChannelPressure => {
                self.data.len() == 2
            }
            _ => true,
        }
    }
}

// Реализация Drop для очистки
impl Drop for MidiEventProcessor {
    fn drop(&mut self) {
        // Отправляем All Notes Off на все активные ноты
        let _ = self.force_all_notes_off();
    }
}