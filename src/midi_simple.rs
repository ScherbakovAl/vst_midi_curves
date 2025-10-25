//! Упрощенная MIDI поддержка для базового тестирования
//! 
//! Это временная упрощенная версия для проверки основной MIDI функциональности
//! без сложных зависимостей и обработки ошибок.

use std::sync::{Arc, Mutex};

// Простые MIDI события
#[derive(Debug, Clone, PartialEq)]
pub enum SimpleMidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8, velocity: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    Other,
}

// Простая статистика MIDI
#[derive(Debug, Clone, Default)]
pub struct SimpleMidiStats {
    pub note_on_count: u64,
    pub note_off_count: u64,
    pub control_change_count: u64,
}

// Простой MIDI менеджер
pub struct SimpleMidiManager {
    curve_processor: Arc<Mutex<crate::curve::BezierCurve>>,
    stats: SimpleMidiStats,
    input_ports: Vec<String>,
    output_ports: Vec<String>,
    is_active: bool,
    
    // Callback для уведомления о MIDI событиях
    event_callback: Option<Arc<dyn Fn(&SimpleMidiEvent) + Send + Sync>>,
}

impl SimpleMidiManager {
    pub fn new(curve_processor: Arc<Mutex<crate::curve::BezierCurve>>) -> Self {
        Self {
            curve_processor,
            stats: SimpleMidiStats::default(),
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            is_active: false,
            event_callback: None,
        }
    }
    
    // Установить callback для MIDI событий
    pub fn set_event_callback(&mut self, callback: Arc<dyn Fn(&SimpleMidiEvent) + Send + Sync>) {
        self.event_callback = Some(callback);
    }
    
    // Вызвать callback если он установлен
    fn notify_callback(&self, event: &SimpleMidiEvent) {
        if let Some(ref callback) = self.event_callback {
            callback(event);
        }
    }
    
    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.is_active = true;
        println!("✅ Простой MIDI менеджер запущен");
        Ok(())
    }
    
    pub fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.is_active = false;
        println!("🛑 Простой MIDI менеджер остановлен");
        Ok(())
    }
    
    pub fn get_input_ports(&self) -> Vec<String> {
        self.input_ports.clone()
    }
    
    pub fn get_output_ports(&self) -> Vec<String> {
        self.output_ports.clone()
    }
    
    pub fn connect_input_port(&mut self, _port_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🎹 Подключен входной порт: {}", _port_name);
        Ok(())
    }
    
    pub fn connect_output_port(&mut self, _port_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🎵 Подключен выходной порт: {}", _port_name);
        Ok(())
    }
    
    pub fn disconnect_all_ports(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔌 Отключены все MIDI порты");
        Ok(())
    }
    
    pub fn get_stats(&self) -> SimpleMidiStats {
        self.stats.clone()
    }
    
    pub fn is_active(&self) -> bool {
        self.is_active
    }
    
    pub fn process_midi_event(&mut self, event: &SimpleMidiEvent) -> Option<SimpleMidiEvent> {
        let result = match event {
            SimpleMidiEvent::NoteOn { channel, note, velocity } => {
                // Применяем кривую к velocity
                let mut curve = self.curve_processor.lock().unwrap();
                let processed_velocity = curve.evaluate(*velocity as f32) as u8;
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
            SimpleMidiEvent::NoteOff { channel: _, note: _, velocity: _ } => {
                self.stats.note_off_count += 1;
                self.notify_callback(event);
                Some(event.clone())
            }
            SimpleMidiEvent::ControlChange { channel: _, controller: _, value: _ } => {
                self.stats.control_change_count += 1;
                self.notify_callback(event);
                Some(event.clone())
            }
            SimpleMidiEvent::Other => {
                self.notify_callback(event);
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
        let test_event = SimpleMidiEvent::NoteOn {
            channel: 0,
            note: 60, // Middle C
            velocity: 64, // Средняя громкость
        };
        
        println!("🎵 Генерация тестового MIDI события: {:?}", test_event);
        
        // Обрабатываем событие через кривую
        if let Some(processed_event) = self.process_midi_event(&test_event) {
            println!("✅ Событие обработано: {:?}", processed_event);
        }
        
        Ok(())
    }
}