//! Упрощенная MIDI поддержка для базового тестирования
//!
//! Это временная упрощенная версия для проверки основной MIDI функциональности
//! без сложных зависимостей и обработки ошибок.
//!
//! Поддерживает отдельные кривые для NoteOn и NoteOff событий.
//! Также поддерживает hi-res MIDI с расширенным диапазоном velocity (0-16383).

//! Режимы работы MIDI
#[derive(Debug, Clone, PartialEq)]
pub enum MidiMode {
    /// Стандартный MIDI режим (velocity 0-127)
    Standard,
    /// Hi-res MIDI режим (velocity 0-16383)
    HighResolution,
}

/// Hi-res MIDI события
#[derive(Debug, Clone, PartialEq)]
pub enum HiResMidiEvent {
    /// Hi-res NoteOn с расширенным диапазоном velocity (0-16383)
    HiResNoteOn { channel: u8, note: u8, velocity: u16 },
    /// Hi-res NoteOff с расширенным диапазоном velocity (0-16383)
    HiResNoteOff { channel: u8, note: u8, velocity: u16 },
    /// Регулярное ControlChange для CC#32 (LSB) который используется с CC#01 (MSB)
    ControlChangeLSB { channel: u8, controller: u8, value: u8 },
    /// Обычное ControlChange для CC#01 (MSB)
    ControlChangeMSB { channel: u8, controller: u8, value: u8 },
}

impl HiResMidiEvent {
    /// Получение канала из события
    pub fn get_channel(&self) -> u8 {
        match self {
            HiResMidiEvent::HiResNoteOn { channel, .. } => *channel,
            HiResMidiEvent::HiResNoteOff { channel, .. } => *channel,
            HiResMidiEvent::ControlChangeLSB { channel, .. } => *channel,
            HiResMidiEvent::ControlChangeMSB { channel, .. } => *channel,
        }
    }
}

use std::sync::{Arc, Mutex};

// Простые MIDI события
#[derive(Debug, Clone, PartialEq)]
pub enum SimpleMidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8, velocity: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    Other,
}

impl SimpleMidiEvent {
    /// Получение канала из события
    pub fn get_channel(&self) -> u8 {
        match self {
            SimpleMidiEvent::NoteOn { channel, .. } => *channel,
            SimpleMidiEvent::NoteOff { channel, .. } => *channel,
            SimpleMidiEvent::ControlChange { channel, .. } => *channel,
            SimpleMidiEvent::Other => 0,
        }
    }
    
    /// Получение ноты из события
    pub fn get_note(&self) -> u8 {
        match self {
            SimpleMidiEvent::NoteOn { note, .. } => *note,
            SimpleMidiEvent::NoteOff { note, .. } => *note,
            SimpleMidiEvent::ControlChange { controller, .. } => *controller,
            SimpleMidiEvent::Other => 60, // Middle C по умолчанию
        }
    }
}

// Простая статистика MIDI
#[derive(Debug, Clone, Default)]
pub struct SimpleMidiStats {
    pub note_on_count: u64,
    pub note_off_count: u64,
    pub control_change_count: u64,
    pub hi_res_note_on_count: u64,
    pub hi_res_note_off_count: u64,
}

// Буфер для хранения частей hi-res MIDI сообщений
#[derive(Debug, Clone)]
struct HiResMidiBuffer {
    msb_controller: Option<u8>, // CC#01 (MSB)
    lsb_controller: Option<u8>, // CC#32 (LSB)
    msb_velocity: Option<u8>,   // Velocity MSB (0x01)
    lsb_velocity: Option<u8>,   // Velocity LSB (0x21)
    last_channel: u8,
}

// Простой MIDI менеджер
pub struct SimpleMidiManager {
    dual_curve_processor: Arc<Mutex<crate::curve::DualCurve>>,
    stats: SimpleMidiStats,
    input_ports: Vec<String>,
    output_ports: Vec<String>,
    is_active: bool,
    
    // Текущий режим MIDI
    current_mode: MidiMode,
    
    // Буфер для hi-res сообщений
    hi_res_buffer: HiResMidiBuffer,
    
    // Callback для уведомления о MIDI событиях
    event_callback: Option<Arc<dyn Fn(&SimpleMidiEvent) + Send + Sync>>,
    
    // Callback для hi-res MIDI событий
    hi_res_callback: Option<Arc<dyn Fn(&HiResMidiEvent) + Send + Sync>>,
}

impl SimpleMidiManager {
    pub fn new(dual_curve_processor: Arc<Mutex<crate::curve::DualCurve>>) -> Self {
        Self {
            dual_curve_processor,
            stats: SimpleMidiStats::default(),
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            is_active: false,
            current_mode: MidiMode::Standard,
            hi_res_buffer: HiResMidiBuffer {
                msb_controller: None,
                lsb_controller: None,
                msb_velocity: None,
                lsb_velocity: None,
                last_channel: 0,
            },
            event_callback: None,
            hi_res_callback: None,
        }
    }
    
    // Установить callback для MIDI событий
    pub fn set_event_callback(&mut self, callback: Arc<dyn Fn(&SimpleMidiEvent) + Send + Sync>) {
        self.event_callback = Some(callback);
    }
    
    // Установить callback для hi-res MIDI событий
    pub fn set_hi_res_callback(&mut self, callback: Arc<dyn Fn(&HiResMidiEvent) + Send + Sync>) {
        self.hi_res_callback = Some(callback);
    }
    
    // Вызвать callback если он установлен
    fn notify_callback(&self, event: &SimpleMidiEvent) {
        if let Some(ref callback) = self.event_callback {
            callback(event);
        }
    }
    
    // Вызвать hi-res callback если он установлен
    fn notify_hi_res_callback(&self, event: &HiResMidiEvent) {
        if let Some(ref callback) = self.hi_res_callback {
            callback(event);
        }
    }
    
    /// Установить режим работы MIDI
    pub fn set_midi_mode(&mut self, mode: MidiMode) {
        self.current_mode = mode;
        // Очищаем буфер при переключении режима
        self.hi_res_buffer = HiResMidiBuffer {
            msb_controller: None,
            lsb_controller: None,
            msb_velocity: None,
            lsb_velocity: None,
            last_channel: 0,
        };
    }
    
    /// Получить текущий режим MIDI
    pub fn get_midi_mode(&self) -> MidiMode {
        self.current_mode.clone()
    }
    
    /// Обработка ControlChange сообщения для hi-res MIDI
    pub fn process_control_change(&mut self, channel: u8, controller: u8, value: u8) -> Option<HiResMidiEvent> {
        match (controller, self.current_mode.clone()) {
            (0x58, MidiMode::HighResolution) => {
                // CC#88 (Fractional Velocity) - дробная часть velocity
                self.hi_res_buffer.lsb_velocity = Some(value);
                self.hi_res_buffer.last_channel = channel;
                
                // Если у нас есть и целая и дробная части, создаем hi-res событие
                if let (Some(msb), Some(lsb)) = (self.hi_res_buffer.msb_velocity, self.hi_res_buffer.lsb_velocity) {
                    let velocity = ((msb as u16) << 7) | (lsb as u16);
                    let note = 60; // Middle C по умолчанию
                    
                    self.hi_res_buffer.msb_velocity = None;
                    self.hi_res_buffer.lsb_velocity = None;
                    
                    self.stats.hi_res_note_on_count += 1;
                    let hi_res_event = HiResMidiEvent::HiResNoteOn {
                        channel,
                        note,
                        velocity,
                    };
                    self.notify_hi_res_callback(&hi_res_event);
                    Some(hi_res_event)
                } else {
                    Some(HiResMidiEvent::ControlChangeLSB { channel, controller, value })
                }
            }
            (0x70, MidiMode::HighResolution) => {
                // CC#112 (Integer Velocity) - целая часть velocity
                self.hi_res_buffer.msb_velocity = Some(value);
                self.hi_res_buffer.last_channel = channel;
                
                // Если у нас есть и целая и дробная части, создаем hi-res событие
                if let (Some(msb), Some(lsb)) = (self.hi_res_buffer.msb_velocity, self.hi_res_buffer.lsb_velocity) {
                    let velocity = ((msb as u16) << 7) | (lsb as u16);
                    let note = 60; // Middle C по умолчанию
                    
                    self.hi_res_buffer.msb_velocity = None;
                    self.hi_res_buffer.lsb_velocity = None;
                    
                    self.stats.hi_res_note_on_count += 1;
                    let hi_res_event = HiResMidiEvent::HiResNoteOn {
                        channel,
                        note,
                        velocity,
                    };
                    self.notify_hi_res_callback(&hi_res_event);
                    Some(hi_res_event)
                } else {
                    Some(HiResMidiEvent::ControlChangeMSB { channel, controller, value })
                }
            }
            (0x01, MidiMode::HighResolution) => {
                // CC#01 (MSB) для Velocity (альтернативный стандарт)
                self.hi_res_buffer.msb_velocity = Some(value);
                self.hi_res_buffer.last_channel = channel;
                
                // Если у нас есть и MSB и LSB, создаем hi-res событие
                if let (Some(msb), Some(lsb)) = (self.hi_res_buffer.msb_velocity, self.hi_res_buffer.lsb_velocity) {
                    let velocity = ((msb as u16) << 7) | (lsb as u16);
                    let note = 60; // Middle C по умолчанию
                    
                    self.hi_res_buffer.msb_velocity = None;
                    self.hi_res_buffer.lsb_velocity = None;
                    
                    self.stats.hi_res_note_on_count += 1;
                    let hi_res_event = HiResMidiEvent::HiResNoteOn {
                        channel,
                        note,
                        velocity,
                    };
                    self.notify_hi_res_callback(&hi_res_event);
                    Some(hi_res_event)
                } else {
                    Some(HiResMidiEvent::ControlChangeMSB { channel, controller, value })
                }
            }
            (0x21, MidiMode::HighResolution) => {
                // CC#33 (LSB) для Velocity (альтернативный стандарт)
                self.hi_res_buffer.lsb_velocity = Some(value);
                self.hi_res_buffer.last_channel = channel;
                
                // Если у нас есть и MSB и LSB, создаем hi-res событие
                if let (Some(msb), Some(lsb)) = (self.hi_res_buffer.msb_velocity, self.hi_res_buffer.lsb_velocity) {
                    let velocity = ((msb as u16) << 7) | (lsb as u16);
                    let note = 60; // Middle C по умолчанию
                    
                    self.hi_res_buffer.msb_velocity = None;
                    self.hi_res_buffer.lsb_velocity = None;
                    
                    self.stats.hi_res_note_on_count += 1;
                    let hi_res_event = HiResMidiEvent::HiResNoteOn {
                        channel,
                        note,
                        velocity,
                    };
                    self.notify_hi_res_callback(&hi_res_event);
                    Some(hi_res_event)
                } else {
                    Some(HiResMidiEvent::ControlChangeLSB { channel, controller, value })
                }
            }
            _ => {
                // Обычные ControlChange для стандартного MIDI
                if let Some(ref callback) = self.event_callback {
                    let event = SimpleMidiEvent::ControlChange { channel, controller, value };
                    callback(&event);
                }
                None
            }
        }
    }
    
    /// Обработка velocity в hi-res режиме
    pub fn process_hi_res_velocity(&mut self, velocity: u16) -> u16 {
        // Правильная обработка hi-res velocity (0-16383):
        // Применяем кривую напрямую к 14-битному диапазону без потери точности
        
        let mut dual_curve = self.dual_curve_processor.lock().unwrap();
        
        // Для линейной кривой сохраняем значение точно
        // Проверяем, является ли кривая линейной (только начальная и конечная точки)
        if dual_curve.note_on_curve.control_points.len() == 2 {
            // Точная проверка на линейность: только (0,0) и (127,127)
            let is_linear = dual_curve.note_on_curve.control_points[0].position.0 == 0.0 &&
                           dual_curve.note_on_curve.control_points[0].position.1 == 0.0 &&
                           dual_curve.note_on_curve.control_points[1].position.0 == 127.0 &&
                           dual_curve.note_on_curve.control_points[1].position.1 == 127.0;
            
            if is_linear {
                return velocity; // Сохраняем точное значение для линейной кривой
            }
        }
        
        // Для нелинейных кривых применяем их напрямую к 14-битному диапазону
        // Масштабируем 14-битное значение (0-16383) к диапазону кривой (0-127)
        let velocity_7bit = (velocity as f32 / 16383.0 * 127.0).clamp(0.0, 127.0);
        
        // Применяем кривую Безье к 7-битному значению
        let processed_7bit = dual_curve.note_on_curve.evaluate(velocity_7bit);
        
        // Восстанавливаем обратно к 14-битному диапазону
        let processed_velocity = (processed_7bit as f32 / 127.0 * 16383.0).round() as u16;
        
        processed_velocity
    }
    
    /// Генерация двух MIDI сообщений из обработанного hi-res velocity
    /// Согласно описанию:
    /// Первое сообщение: 9, 176, 88, fractional_part (дробная часть скорости)
    /// Второе сообщение: 9, 144, note, integer_part (цельная часть скорости)
    pub fn generate_hi_res_midi_messages(&self, velocity: u16, channel: u8, note: u8) -> Vec<Vec<u8>> {
        let msb = (velocity >> 7) as u8;         // 7-бит MSB (целая часть)
        let lsb = (velocity & 0x7F) as u8;       // 7-бит LSB (дробная часть)
        
        vec![
            vec![0x90 | (channel & 0x0F), 0xB0, 0x58, lsb], // Первое: CC сообщение для дробной части
            vec![0x90 | (channel & 0x0F), note, msb, 0x00], // Второе: NoteOn для целой части (с padding)
        ]
    }
    
    /// Генерация тестовых hi-res MIDI сообщений для проверки исправлений
    pub fn generate_hi_res_test_messages(&self) -> Vec<Vec<u8>> {
        // Тестируем значение из примера: 20 (цельная) и 10 (дробная)
        let test_velocity: u16 = (20 << 7) | 10; // 20*128 + 10 = 2570
        
        // Применяем кривую используя тот же алгоритм что и в process_hi_res_velocity
        let processed_velocity = {
            let mut dual_curve = self.dual_curve_processor.lock().unwrap();
            
            // Для линейной кривой сохраняем значение точно
            if dual_curve.note_on_curve.control_points.len() == 2 {
                let is_linear = dual_curve.note_on_curve.control_points[0].position.0 == 0.0 &&
                               dual_curve.note_on_curve.control_points[0].position.1 == 0.0 &&
                               dual_curve.note_on_curve.control_points[1].position.0 == 127.0 &&
                               dual_curve.note_on_curve.control_points[1].position.1 == 127.0;
                
                if is_linear {
                    return self.generate_hi_res_midi_messages(test_velocity, 0, 56);
                }
            }
            
            // Для нелинейных кривых применяем тот же алгоритм
            let velocity_7bit = (test_velocity as f32 / 16383.0 * 127.0).clamp(0.0, 127.0);
            let processed_7bit = dual_curve.note_on_curve.evaluate(velocity_7bit);
            (processed_7bit as f32 / 127.0 * 16383.0).round() as u16
        };
        
        self.generate_hi_res_midi_messages(processed_velocity, 0, 56)
    }
    
    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.is_active = true;
        Ok(())
    }
    
    pub fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.is_active = false;
        Ok(())
    }
    
    pub fn get_input_ports(&self) -> Vec<String> {
        self.input_ports.clone()
    }
    
    pub fn get_output_ports(&self) -> Vec<String> {
        self.output_ports.clone()
    }
    
    pub fn connect_input_port(&mut self, _port_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
    
    pub fn connect_output_port(&mut self, _port_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
    
    pub fn disconnect_all_ports(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
    
    pub fn get_stats(&self) -> SimpleMidiStats {
        self.stats.clone()
    }
    
    /// Очистка буфера hi-res MIDI
    pub fn clear_hi_res_buffer(&mut self) {
        self.hi_res_buffer = HiResMidiBuffer {
            msb_controller: None,
            lsb_controller: None,
            msb_velocity: None,
            lsb_velocity: None,
            last_channel: 0,
        };
    }
    
    /// Проверка наличия данных в буфере hi-res MIDI
    pub fn has_hi_res_data(&self) -> bool {
        self.hi_res_buffer.msb_velocity.is_some() ||
        self.hi_res_buffer.lsb_velocity.is_some() ||
        self.hi_res_buffer.msb_controller.is_some() ||
        self.hi_res_buffer.lsb_controller.is_some()
    }
    
    /// Принудительная генерация hi-res события из буфера
    pub fn flush_hi_res_buffer(&mut self) -> Option<HiResMidiEvent> {
        if let (Some(msb), Some(lsb)) = (self.hi_res_buffer.msb_velocity, self.hi_res_buffer.lsb_velocity) {
            let velocity = ((msb as u16) << 7) | (lsb as u16);
            let channel = self.hi_res_buffer.last_channel;
            
            self.hi_res_buffer.msb_velocity = None;
            self.hi_res_buffer.lsb_velocity = None;
            
            self.stats.hi_res_note_on_count += 1;
            let hi_res_event = HiResMidiEvent::HiResNoteOn {
                channel,
                note: 60,
                velocity,
            };
            self.notify_hi_res_callback(&hi_res_event);
            Some(hi_res_event)
        } else {
            None
        }
    }
    
    /// Генерация тестового hi-res MIDI события
    pub fn generate_hi_res_test(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.current_mode != MidiMode::HighResolution {
            return Err("Необходимо переключиться в hi-res режим".into());
        }
        
        println!("🎵 Тестирование hi-res MIDI:");
        
        // Тестируем высокое разрешение velocity
        let test_velocity: u16 = 8192; // Среднее значение в диапазоне 0-16383
        
        let msb = (test_velocity >> 7) as u8;
        let lsb = (test_velocity & 0x7F) as u8;
        
        println!("  Тест velocity: {} (MSB: {}, LSB: {})", test_velocity, msb, lsb);
        
        // Симулируем получение MSB
        self.hi_res_buffer.msb_velocity = Some(msb);
        self.hi_res_buffer.last_channel = 0;
        
        // Симулируем получение LSB
        self.hi_res_buffer.lsb_velocity = Some(lsb);
        
        // Попытка сгенерировать событие
        if let Some(hi_res_event) = self.flush_hi_res_buffer() {
            println!("  → Сгенерированное hi-res событие: {:?}", hi_res_event);
        } else {
            println!("  → Не удалось сгенерировать событие (буфер неполный)");
        }
        
        Ok(())
    }
    
    pub fn is_active(&self) -> bool {
        self.is_active
    }
    
pub fn process_midi_event(&mut self, event: &SimpleMidiEvent) -> Option<SimpleMidiEvent> {
        let result = match event {
            SimpleMidiEvent::NoteOn { channel, note, velocity } => {
                // Применяем кривую NoteOn к velocity
                let mut dual_curve = self.dual_curve_processor.lock().unwrap();
                let processed_velocity = dual_curve.process_note_on_velocity(*velocity);
                self.stats.note_on_count += 1;
                
                let processed_event = SimpleMidiEvent::NoteOn {
                    channel: *channel,
                    note: *note,
                    velocity: processed_velocity,
                };
                
                // Уведомляем о входящем событии
                self.notify_callback(event);
                // Уведомляем об обработанном событии
                self.notify_callback(&processed_event);
                
                Some(processed_event)
            }
            SimpleMidiEvent::NoteOff { channel, note, velocity } => {
                // Применяем кривую NoteOff к velocity
                let mut dual_curve = self.dual_curve_processor.lock().unwrap();
                let processed_velocity = dual_curve.process_note_off_velocity(*velocity);
                self.stats.note_off_count += 1;
                
                let processed_event = SimpleMidiEvent::NoteOff {
                    channel: *channel,
                    note: *note,
                    velocity: processed_velocity,
                };
                
                // Уведомляем о входящем событии
                self.notify_callback(event);
                // Уведомляем об обработанном событии
                self.notify_callback(&processed_event);
                
                Some(processed_event)
            }
            SimpleMidiEvent::ControlChange { channel, controller, value } => {
                // Специальная обработка для hi-res MIDI
                if let Some(hi_res_event) = self.process_control_change(*channel, *controller, *value) {
                    self.stats.control_change_count += 1;
                    
                    // Применяем кривую к hi-res событию
                    let processed_hi_res_event = match hi_res_event {
                        HiResMidiEvent::HiResNoteOn { channel, note, velocity } => {
                            // Применяем кривую к hi-res velocity
                            let processed_velocity = self.process_hi_res_velocity(velocity);
                            HiResMidiEvent::HiResNoteOn {
                                channel,
                                note,
                                velocity: processed_velocity,
                            }
                        }
                        HiResMidiEvent::HiResNoteOff { channel, note, velocity } => {
                            // Применяем кривую к hi-res velocity
                            let processed_velocity = self.process_hi_res_velocity(velocity);
                            HiResMidiEvent::HiResNoteOff {
                                channel,
                                note,
                                velocity: processed_velocity,
                            }
                        }
                        _ => hi_res_event.clone(),
                    };
                    
                    // Уведомляем о событии
                    self.notify_hi_res_callback(&processed_hi_res_event);
                    Some(event.clone())
                } else {
                    self.stats.control_change_count += 1;
                    self.notify_callback(event);
                    Some(event.clone())
                }
            }
            SimpleMidiEvent::Other => {
                self.notify_callback(event);
                Some(event.clone())
            },
        };
        
        result
    }
    
    /// Обработка hi-res MIDI событий с применением кривых
    pub fn process_hi_res_midi_event(&mut self, event: &HiResMidiEvent) -> Option<HiResMidiEvent> {
        let result = match event {
            HiResMidiEvent::HiResNoteOn { channel, note, velocity } => {
                // Применяем кривую к hi-res velocity
                let processed_velocity = self.process_hi_res_velocity(*velocity);
                self.stats.hi_res_note_on_count += 1;
                
                let processed_event = HiResMidiEvent::HiResNoteOn {
                    channel: *channel,
                    note: *note,
                    velocity: processed_velocity,
                };
                
                // Уведомляем о входящем и обработанном событии
                self.notify_hi_res_callback(event);
                self.notify_hi_res_callback(&processed_event);
                
                Some(processed_event)
            }
            HiResMidiEvent::HiResNoteOff { channel, note, velocity } => {
                // Применяем кривую к hi-res velocity
                let processed_velocity = self.process_hi_res_velocity(*velocity);
                self.stats.hi_res_note_off_count += 1;
                
                let processed_event = HiResMidiEvent::HiResNoteOff {
                    channel: *channel,
                    note: *note,
                    velocity: processed_velocity,
                };
                
                // Уведомляем о входящем и обработанном событии
                self.notify_hi_res_callback(event);
                self.notify_hi_res_callback(&processed_event);
                
                Some(processed_event)
            }
            HiResMidiEvent::ControlChangeMSB { channel, controller, value } => {
                self.stats.control_change_count += 1;
                self.notify_hi_res_callback(event);
                Some(event.clone())
            }
            HiResMidiEvent::ControlChangeLSB { channel, controller, value } => {
                self.stats.control_change_count += 1;
                self.notify_hi_res_callback(event);
                Some(event.clone())
            },
        };
        
        result
    }
    
    pub fn add_input_port(&mut self, port_name: String) {
        self.input_ports.push(port_name);
    }
    
    pub fn add_output_port(&mut self, port_name: String) {
        self.output_ports.push(port_name);
    }
    
    pub fn refresh_ports(&mut self) {
        // Добавляем фиктивные порты для тестирования
        self.input_ports = vec![
            "Test Input 1".to_string(),
            "MIDI Keyboard".to_string(),
        ];
        self.output_ports = vec![
            "Test Output 1".to_string(),
            "Virtual Synth".to_string(),
        ];
    }
    
// Генерация тестового MIDI события
    pub fn generate_test_event(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🎵 Тестирование обеих кривых:");
        
        // Тестируем NoteOn событие
        let note_on_event = SimpleMidiEvent::NoteOn {
            channel: 0,
            note: 60, // Middle C
            velocity: 64, // Средняя громкость
        };
        
        println!("  NoteOn событие: {:?}", note_on_event);
        if let Some(processed_note_on) = self.process_midi_event(&note_on_event) {
            println!("  → Обработанное NoteOn: {:?}", processed_note_on);
        }
        
        // Тестируем NoteOff событие
        let note_off_event = SimpleMidiEvent::NoteOff {
            channel: 0,
            note: 60,
            velocity: 64,
        };
        
        println!("  NoteOff событие: {:?}", note_off_event);
        if let Some(processed_note_off) = self.process_midi_event(&note_off_event) {
            println!("  → Обработанное NoteOff: {:?}", processed_note_off);
        }
        
        Ok(())
    }
}