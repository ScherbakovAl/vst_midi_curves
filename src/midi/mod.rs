//! MIDI менеджер для обработки MIDI событий и коммуникации с внешними устройства
//! 
//! Этот модуль отвечает за:
//! - Обнаружение и управление MIDI портами
//! - Получение и обработку MIDI событий
//! - Применение кривых Безье к velocity MIDI нот
//! - Отправку обработанных событий на выходные порты

use std::sync::{Arc, Mutex};

// Подмодули MIDI системы
pub mod ports;
pub mod processor;
pub mod device_manager;

use ports::{MidiInputPort, MidiOutputPort, MidiPortManager};
use processor::MidiEventProcessor;
use device_manager::DeviceManager;

// MIDI события
#[derive(Debug, Clone)]
pub enum MidiEvent {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
        timestamp: std::time::Instant,
    },
    NoteOff {
        channel: u8,
        note: u8,
        velocity: u8,
        timestamp: std::time::Instant,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
        timestamp: std::time::Instant,
    },
    ProgramChange {
        channel: u8,
        program: u8,
        timestamp: std::time::Instant,
    },
    PitchBend {
        channel: u8,
        value: u16,
        timestamp: std::time::Instant,
    },
    // Hi-res MIDI события
    HiResNoteOn {
        channel: u8,
        note: u8,
        velocity_msb: u8,
        velocity_lsb: u8,
        timestamp: std::time::Instant,
    },
    HiResNoteOff {
        channel: u8,
        note: u8,
        velocity_msb: u8,
        velocity_lsb: u8,
        timestamp: std::time::Instant,
    },
    HiResControlChange {
        channel: u8,
        controller: u8,
        value_msb: u8,
        value_lsb: u8,
        timestamp: std::time::Instant,
    },
    // Системные события
    Start,
    Stop,
    Continue,
    Reset,
}

// Callback для уведомления о MIDI событиях
pub type MidiEventCallback = Arc<dyn Fn(MidiEvent) + Send + Sync>;

/// Статистика MIDI активности
#[derive(Debug, Clone, Default)]
pub struct MidiStats {
    pub note_on_count: u64,
    pub note_off_count: u64,
    pub control_change_count: u64,
    pub program_change_count: u64,
    pub pitch_bend_count: u64,
    pub errors_count: u64,
}

impl MidiStats {
    /// Создание новой статистики с нулевыми значениями
    pub fn new() -> Self {
        Self {
            note_on_count: 0,
            note_off_count: 0,
            control_change_count: 0,
            program_change_count: 0,
            pitch_bend_count: 0,
            errors_count: 0,
        }
    }
    
    /// Сброс статистики к нулевым значениям
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

// Основной MIDI менеджер
pub struct MidiManager {
    // Управление портами
    port_manager: Arc<Mutex<MidiPortManager>>,
    
    // Обработка событий
    event_processor: Arc<Mutex<MidiEventProcessor>>,
    
    // Управление устройствами
    device_manager: Arc<Mutex<DeviceManager>>,
    
    // Двойная кривая для обработки NoteOn и NoteOff velocity
    dual_curve_processor: Arc<Mutex<crate::curve::DualCurve>>,
    
    // Callbacks для UI
    input_callback: Option<MidiEventCallback>,
    output_callback: Option<MidiEventCallback>,
    
    // Статистика
    stats: Arc<Mutex<MidiStats>>,
    
    // Флаги состояния
    is_running: bool,
    input_enabled: bool,
    output_enabled: bool,
}

impl MidiManager {
    pub fn new(dual_curve_processor: Arc<Mutex<crate::curve::DualCurve>>) -> Self {
        Self {
            port_manager: Arc::new(Mutex::new(MidiPortManager::new())),
            event_processor: Arc::new(Mutex::new(MidiEventProcessor::new())),
            device_manager: Arc::new(Mutex::new(DeviceManager::new())),
            dual_curve_processor,
            input_callback: None,
            output_callback: None,
            stats: Arc::new(Mutex::new(MidiStats::new())),
            is_running: false,
            input_enabled: false,
            output_enabled: false,
        }
    }
    
    /// Запуск MIDI менеджера
    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_running {
            return Ok(());
        }
        
        // Сканируем доступные MIDI порты
        let mut device_manager = self.device_manager.lock().unwrap();
        device_manager.scan_ports()?;
        
        self.is_running = true;
        Ok(())
    }
    
    /// Остановка MIDI менеджера
    pub fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.is_running {
            return Ok(());
        }
        
        // Отключаем все порты
        self.disconnect_all_ports()?;
        
        // Сбрасываем статистику
        self.stats.lock().unwrap().reset();
        
        self.is_running = false;
        Ok(())
    }
    
    /// Получение списка входных MIDI портов
    pub fn get_input_ports(&self) -> Vec<String> {
        self.device_manager
            .lock()
            .unwrap()
            .get_input_port_names()
    }
    
    /// Получение списка выходных MIDI портов
    pub fn get_output_ports(&self) -> Vec<String> {
        self.device_manager
            .lock()
            .unwrap()
            .get_output_port_names()
    }
    
    /// Подключение к входному MIDI порту
    pub fn connect_input_port(&mut self, port_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        if !self.is_running {
            return Err("MIDI менеджер не запущен".into());
        }
        
        let mut port_manager = self.port_manager.lock().unwrap();
        
        // Создаем callback для обработки MIDI событий
        let dual_curve_processor = Arc::clone(&self.dual_curve_processor);
        let event_processor = Arc::clone(&self.event_processor);
        let stats = Arc::clone(&self.stats);
        let output_callback = self.output_callback.clone();
        
        let port_manager_for_callback = Arc::clone(&self.port_manager);
        
        let callback = Arc::new(Mutex::new(move |data: &[u8], timestamp: u64| {
            Self::process_midi_input(
                data,
                timestamp,
                &dual_curve_processor,
                &event_processor,
                &stats,
                &output_callback,
                &port_manager_for_callback,
            );
        }));
        
        // Создаем новый входной порт с callback
        let mut input_port = MidiInputPort::new(port_name)?;
        input_port.set_callback(callback)?;
        port_manager.set_input_port(input_port);
        self.input_enabled = true;
        
        Ok(())
    }
    
    /// Подключение к выходному MIDI порту
    pub fn connect_output_port(&mut self, port_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        if !self.is_running {
            return Err("MIDI менеджер не запущен".into());
        }
        
        let mut port_manager = self.port_manager.lock().unwrap();
        
        // Создаем новый выходной порт
        let output_port = MidiOutputPort::new(port_name)?;
        port_manager.set_output_port(output_port);
        self.output_enabled = true;
        
        Ok(())
    }
    
    /// Отключение всех MIDI портов
    pub fn disconnect_all_ports(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut port_manager = self.port_manager.lock().unwrap();
        port_manager.disconnect_all().unwrap_or_default();
        
        self.input_enabled = false;
        self.output_enabled = false;
        
        Ok(())
    }
    
    /// Отключение входного MIDI порта
    pub fn disconnect_input_port(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut port_manager = self.port_manager.lock().unwrap();
        
        // Отключаем все входные порты напрямую
        for (_port_id, input_port) in &mut port_manager.input_ports {
            input_port.disconnect();
        }
        
        port_manager.input_ports.clear();
        self.input_enabled = false;
        
        Ok(())
    }
    
    /// Отключение выходного MIDI порта
    pub fn disconnect_output_port(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut port_manager = self.port_manager.lock().unwrap();
        
        // Отключаем все выходные порты напрямую
        for (_port_id, output_port) in &mut port_manager.output_ports {
            output_port.disconnect();
        }
        
        port_manager.output_ports.clear();
        self.output_enabled = false;
        
        Ok(())
    }
    
    /// Установка callback для входящих событий
    pub fn set_input_callback(&mut self, callback: MidiEventCallback) {
        self.input_callback = Some(callback);
    }
    
    /// Установка callback для исходящих событий
    pub fn set_output_callback(&mut self, callback: MidiEventCallback) {
        self.output_callback = Some(callback);
    }
    
    /// Получение текущей статистики
    pub fn get_stats(&self) -> MidiStats {
        self.stats.lock().unwrap().clone()
    }
    
    /// Отправка MIDI данных на подключенные выходные порты
    pub fn send_midi_data(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let mut port_manager = self.port_manager.lock().unwrap();
        
        let mut sent = false;
        for (_port_id, port) in &mut port_manager.output_ports {
            if port.is_connected() {
                match port.send_midi(data) {
                    Ok(_) => {
                        sent = true;
                    }
                    Err(_) => {
                        // Silent error handling
                    }
                }
            }
        }
        
        if !sent {
            return Err("Нет подключенных выходных MIDI портов".into());
        }
        
        Ok(())
    }
    
    /// Проверка состояния MIDI менеджера
    pub fn is_active(&self) -> bool {
        self.is_running && (self.input_enabled || self.output_enabled)
    }
    
    /// Обработка входящего MIDI сообщения
    fn process_midi_input(
        data: &[u8],
        timestamp: u64,
        dual_curve_processor: &Arc<Mutex<crate::curve::DualCurve>>,
        event_processor: &Arc<Mutex<MidiEventProcessor>>,
        stats: &Arc<Mutex<MidiStats>>,
        output_callback: &Option<MidiEventCallback>,
        port_manager: &Arc<Mutex<MidiPortManager>>,
    ) {
        if data.len() < 3 {
            return;
        }
        
        let status = data[0];
        let controller = data[1];
        let value = data[2];
        
        // Проверяем hi-res MIDI сообщения
        if status == 0xB9 && controller == 88 { // CC#88 + LSB на канале 9
            // Первое сообщение hi-res: дробная часть velocity
            
            // Обрабатываем как HiResControlChange
            let hi_res_event = MidiEvent::HiResControlChange {
                channel: status & 0x0F,
                controller: 88, // CC#88
                value_msb: 0, // Будет заполнено из следующего сообщения
                value_lsb: value,
                timestamp: std::time::Instant::now(),
            };
            
            let processed_event = Self::process_velocity(hi_res_event, dual_curve_processor);
            Self::send_processed_event(&processed_event, output_callback, port_manager);
            
            // Обновляем статистику
            {
                let mut stats_mut = stats.lock().unwrap();
                stats_mut.control_change_count += 1;
            }
            
        } else if status == 0x99 && data.len() == 3 { // NoteOn на канале 9
            // Второе сообщение hi-res: NoteOn с цельной частью velocity
            let note = data[1];
            let velocity_msb = data[2];
            
            // Создаем hi-res NoteOn событие
            let hi_res_event = MidiEvent::HiResNoteOn {
                channel: status & 0x0F,
                note: note,
                velocity_msb: velocity_msb,
                velocity_lsb: 0, // Будет заполнено из предыдущего CC сообщения
                timestamp: std::time::Instant::now(),
            };
            
            let processed_event = Self::process_velocity(hi_res_event, dual_curve_processor);
            Self::send_processed_event(&processed_event, output_callback, port_manager);
            
            // Обновляем статистику
            {
                let mut stats_mut = stats.lock().unwrap();
                stats_mut.note_on_count += 1;
            }
            
        } else {
            // Обычное MIDI сообщение
            if let Ok(midi_event) = MidiEvent::from_midi_data(data, timestamp) {
                let processed_event = Self::process_velocity(midi_event, dual_curve_processor);
                Self::send_processed_event(&processed_event, output_callback, port_manager);
                
                // Обновляем статистику
                {
                    let mut stats_mut = stats.lock().unwrap();
                    Self::update_stats(&processed_event, &mut stats_mut);
                }
            }
        }
        
        // Также обрабатываем через event processor для дополнительной логики
        if let Ok(midi_event) = MidiEvent::from_midi_data(data, timestamp) {
            let processed_event = Self::process_velocity(midi_event, dual_curve_processor);
            let output_data = processed_event.to_midi_data();
            let _ = event_processor.lock().unwrap().process_midi_data(&output_data, timestamp);
        }
    }
    
    /// Отправка обработанного события
    fn send_processed_event(
        event: &MidiEvent,
        output_callback: &Option<MidiEventCallback>,
        port_manager: &Arc<Mutex<MidiPortManager>>,
    ) {
        // Отправляем через callback
        if let Some(ref callback) = *output_callback {
            callback(event.clone());
        }
        
        // Отправляем на выходные порты
        let output_data = event.to_midi_data();
        
        let mut port_manager = port_manager.lock().unwrap();
        for (_port_id, output_port) in &mut port_manager.output_ports {
            if output_port.is_connected() {
                let _ = output_port.send_midi(&output_data);
            }
        }
    }
    
    /// Применение соответствующих кривых к velocity событий
    fn process_velocity(
        event: MidiEvent,
        dual_curve_processor: &Arc<Mutex<crate::curve::DualCurve>>
    ) -> MidiEvent {
        match event {
            MidiEvent::NoteOn { channel, note, velocity, timestamp } => {
                let mut dual_curve = dual_curve_processor.lock().unwrap();
                let processed_velocity = dual_curve.process_note_on_velocity(velocity);
                
                MidiEvent::NoteOn {
                    channel,
                    note,
                    velocity: processed_velocity,
                    timestamp,
                }
            }
            MidiEvent::NoteOff { channel, note, velocity, timestamp } => {
                let mut dual_curve = dual_curve_processor.lock().unwrap();
                let processed_velocity = dual_curve.process_note_off_velocity(velocity);
                
                MidiEvent::NoteOff {
                    channel,
                    note,
                    velocity: processed_velocity,
                    timestamp,
                }
            }
            // Hi-Res MIDI обработка - обрабатываем дробную часть через кривую!
            MidiEvent::HiResNoteOn { channel, note, velocity_msb, velocity_lsb, timestamp } => {
                let mut dual_curve = dual_curve_processor.lock().unwrap();
                
                // Объединяем MSB и LSB для получения 14-битного значения
                let combined_velocity = ((velocity_msb as u16) << 7) | (velocity_lsb as u16);
                
                // Нормализуем к 0-1, обрабатываем через кривую, восстанавливаем к 14-битному
                let normalized_input = combined_velocity as f32 / 16383.0;
                let normalized_output = dual_curve.note_on_curve.evaluate(normalized_input);
                let processed_combined = (normalized_output * 16383.0).round() as u16;
                
                // Разделяем обратно на MSB и LSB
                let processed_msb = (processed_combined >> 7) as u8;
                let processed_lsb = (processed_combined & 0x7F) as u8;
                
                MidiEvent::HiResNoteOn {
                    channel,
                    note,
                    velocity_msb: processed_msb,
                    velocity_lsb: processed_lsb,
                    timestamp,
                }
            }
            MidiEvent::HiResNoteOff { channel, note, velocity_msb, velocity_lsb, timestamp } => {
                let mut dual_curve = dual_curve_processor.lock().unwrap();
                
                // Объединяем MSB и LSB для получения 14-битного значения
                let combined_velocity = ((velocity_msb as u16) << 7) | (velocity_lsb as u16);
                
                // Нормализуем к 0-1, обрабатываем через кривую, восстанавливаем к 14-битному
                let normalized_input = combined_velocity as f32 / 16383.0;
                let normalized_output = dual_curve.note_off_curve.evaluate(normalized_input);
                let processed_combined = (normalized_output * 16383.0).round() as u16;
                
                // Разделяем обратно на MSB и LSB
                let processed_msb = (processed_combined >> 7) as u8;
                let processed_lsb = (processed_combined & 0x7F) as u8;
                
                MidiEvent::HiResNoteOff {
                    channel,
                    note,
                    velocity_msb: processed_msb,
                    velocity_lsb: processed_lsb,
                    timestamp,
                }
            }
            MidiEvent::HiResControlChange { channel, controller, value_msb, value_lsb, timestamp } => {
                // Hi-res Control Change тоже обрабатываем через кривую
                let mut dual_curve = dual_curve_processor.lock().unwrap();
                
                // Объединяем MSB и LSB для получения 14-битного значения
                let combined_value = ((value_msb as u16) << 7) | (value_lsb as u16);
                
                // Нормализуем к 0-1, обрабатываем через кривую, восстанавливаем к 14-битному
                let normalized_input = combined_value as f32 / 16383.0;
                let normalized_output = dual_curve.note_on_curve.evaluate(normalized_input); // Используем NoteOn кривую для CC
                let processed_combined = (normalized_output * 16383.0).round() as u16;
                
                // Разделяем обратно на MSB и LSB
                let processed_msb = (processed_combined >> 7) as u8;
                let processed_lsb = (processed_combined & 0x7F) as u8;
                
                MidiEvent::HiResControlChange {
                    channel,
                    controller,
                    value_msb: processed_msb,
                    value_lsb: processed_lsb,
                    timestamp,
                }
            }
            _ => event, // Остальные события не обрабатываем
        }
    }
    
    /// Обновление статистики событий
    fn update_stats(event: &MidiEvent, stats: &mut MidiStats) {
        match event {
            MidiEvent::NoteOn { .. } => stats.note_on_count += 1,
            MidiEvent::NoteOff { .. } => stats.note_off_count += 1,
            MidiEvent::ControlChange { .. } => stats.control_change_count += 1,
            MidiEvent::ProgramChange { .. } => stats.program_change_count += 1,
            MidiEvent::PitchBend { .. } => stats.pitch_bend_count += 1,
            _ => {}
        }
    }
    
    /// Подготовка события для отправки на выход
    fn prepare_output_event(event: &MidiEvent) -> Option<MidiEvent> {
        // В реальной реализации здесь был бы код подготовки для выходного порта
        Some(event.clone())
    }
}

impl Drop for MidiManager {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

// Конвертация MIDI данных в события
impl MidiEvent {
    pub fn from_midi_data(data: &[u8], _timestamp: u64) -> Result<Self, &'static str> {
        if data.is_empty() {
            return Err("Пустые MIDI данные");
        }
        
        let status = data[0];
        let timestamp = std::time::Instant::now();
        
        match status & 0xF0 {
            0x90 => { // Note On
                if data.len() >= 3 && data[2] > 0 {
                    Ok(MidiEvent::NoteOn {
                        channel: status & 0x0F,
                        note: data[1],
                        velocity: data[2],
                        timestamp,
                    })
                } else if data.len() >= 3 {
                    // Note On с velocity 0 = Note Off
                    Ok(MidiEvent::NoteOff {
                        channel: status & 0x0F,
                        note: data[1],
                        velocity: data[2],
                        timestamp,
                    })
                } else {
                    Err("Недостаточно данных для NoteOn")
                }
            }
            0x80 => { // Note Off
                if data.len() >= 3 {
                    Ok(MidiEvent::NoteOff {
                        channel: status & 0x0F,
                        note: data[1],
                        velocity: data[2],
                        timestamp,
                    })
                } else {
                    Err("Недостаточно данных для NoteOff")
                }
            }
            0xB0 => { // Control Change
                if data.len() >= 3 {
                    Ok(MidiEvent::ControlChange {
                        channel: status & 0x0F,
                        controller: data[1],
                        value: data[2],
                        timestamp,
                    })
                } else {
                    Err("Недостаточно данных для ControlChange")
                }
            }
            0xC0 => { // Program Change
                if data.len() >= 2 {
                    Ok(MidiEvent::ProgramChange {
                        channel: status & 0x0F,
                        program: data[1],
                        timestamp,
                    })
                } else {
                    Err("Недостаточно данных для ProgramChange")
                }
            }
            0xE0 => { // Pitch Bend
                if data.len() >= 3 {
                    let value = ((data[2] as u16) << 7) | (data[1] as u16);
                    Ok(MidiEvent::PitchBend {
                        channel: status & 0x0F,
                        value,
                        timestamp,
                    })
                } else {
                    Err("Недостаточно данных для PitchBend")
                }
            }
            0xFA => Ok(MidiEvent::Start), // Start
            0xFC => Ok(MidiEvent::Stop),  // Stop
            0xFB => Ok(MidiEvent::Continue), // Continue
            0xFF => Ok(MidiEvent::Reset), // Reset
            _ => Err("Неизвестный тип MIDI события"),
        }
    }
    
    /// Конвертация события обратно в MIDI данные
    pub fn to_midi_data(&self) -> Vec<u8> {
        match self {
            MidiEvent::NoteOn { channel, note, velocity, .. } => {
                vec![0x90 | (channel & 0x0F), *note, *velocity]
            }
            MidiEvent::NoteOff { channel, note, velocity, .. } => {
                vec![0x80 | (channel & 0x0F), *note, *velocity]
            }
            MidiEvent::ControlChange { channel, controller, value, .. } => {
                vec![0xB0 | (channel & 0x0F), *controller, *value]
            }
            MidiEvent::ProgramChange { channel, program, .. } => {
                vec![0xC0 | (channel & 0x0F), *program]
            }
            MidiEvent::PitchBend { channel, value, .. } => {
                vec![0xE0 | (channel & 0x0F), (value & 0x7F) as u8, ((value >> 7) & 0x7F) as u8]
            }
            // Hi-res MIDI события - генерируем соответствующие сообщения
            MidiEvent::HiResNoteOn { channel, note, velocity_msb, velocity_lsb, .. } => {
                // Генерируем два сообщения: NoteOn + CC для дробной части
                vec![
                    0x90 | (channel & 0x0F), *note, *velocity_msb,
                    0xB0 | (channel & 0x0F), 88, *velocity_lsb
                ]
            }
            MidiEvent::HiResNoteOff { channel, note, velocity_msb, velocity_lsb, .. } => {
                // Генерируем два сообщения: NoteOff + CC для дробной части
                vec![
                    0x80 | (channel & 0x0F), *note, *velocity_msb,
                    0xB0 | (channel & 0x0F), 88, *velocity_lsb
                ]
            }
            MidiEvent::HiResControlChange { channel, controller, value_msb, value_lsb, .. } => {
                // Генерируем два CC сообщения
                vec![
                    0xB0 | (channel & 0x0F), *controller, *value_msb,
                    0xB0 | (channel & 0x0F), 33, *value_lsb
                ]
            }
            MidiEvent::Start => vec![0xFA],
            MidiEvent::Stop => vec![0xFC],
            MidiEvent::Continue => vec![0xFB],
            MidiEvent::Reset => vec![0xFF],
        }
    }
}