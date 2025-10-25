//! Управление MIDI входными и выходными портами
//! 
//! Этот модуль отвечает за:
//! - Создание и управление MIDI входными портами
//! - Создание и управление MIDI выходными портами  
//! - Обработку входящих MIDI данных
//! - Отправку исходящих MIDI данных

use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::error::Error;
use midir::{MidiInput, MidiOutput, MidiInputConnection, MidiOutputConnection};
use midir::Ignore;

// Callback тип для входящих MIDI данных - упрощенный без Arc<Mutex<>>
pub type MidiInputCallback = Arc<Mutex<dyn FnMut(&[u8], u64) + Send>>;

// Информация о MIDI порте
#[derive(Debug, Clone)]
pub struct MidiPortInfo {
    pub id: String,
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
}

impl MidiPortInfo {
    pub fn new(id: String, name: String, is_input: bool, is_output: bool) -> Self {
        Self {
            id,
            name,
            is_input,
            is_output,
        }
    }
    
    pub fn is_virtual(&self) -> bool {
        self.name.contains("Virtual") || self.name.contains("IAC") || self.name.contains("loop")
    }
}

// Входной MIDI порт
pub struct MidiInputPort {
    port_info: MidiPortInfo,
    connection: Option<MidiInputConnection<MidiInputCallback>>,
}

impl MidiInputPort {
    pub fn new(port_name: &str) -> Result<Self, Box<dyn Error>> {
        // Получаем доступные MIDI входные порты
        let midi_input = MidiInput::new("MIDI Curves Input")?;
        
        let ports = midi_input.ports();
        if ports.is_empty() {
            return Err("Не найдены MIDI входные порты".into());
        }
        
        // Ищем порт по имени
        let mut selected_port = None;
        for (i, port) in ports.iter().enumerate() {
            if let Ok(port_name_str) = midi_input.port_name(port) {
                if port_name_str.contains(port_name) {
                    selected_port = Some((i, port.clone()));
                    break;
                }
            }
        }
        
        // Если конкретный порт не найден, используем первый доступный
        let (port_index, port) = match selected_port {
            Some((index, port)) => (index, port),
            None => {
                // Показываем все доступные порты
                println!("🎹 Доступные MIDI входные порты:");
                for (i, port) in ports.iter().enumerate() {
                    if let Ok(port_name) = midi_input.port_name(port) {
                        println!("  {}: {}", i, port_name);
                    }
                }
                
                if ports.is_empty() {
                    return Err("MIDI входные порты не найдены".into());
                }
                
                (0, ports[0].clone())
            }
        };
        
        // Получаем имя выбранного порта
        let port_name = midi_input.port_name(&port)
            .unwrap_or_else(|_| format!("Порт {}", port_index));
        
        let port_info = MidiPortInfo::new(
            format!("input_{}", port_index),
            port_name,
            true,
            false,
        );
        
        println!("✅ Создан входной MIDI порт: {}", port_info.name);
        
        Ok(Self {
            port_info,
            connection: None,
        })
    }
    
    pub fn set_callback(&mut self, callback: MidiInputCallback) -> Result<(), Box<dyn Error>> {
        // Получаем доступ к MIDI порту заново для создания подключения
        let midi_input = MidiInput::new("MIDI Curves Input")?;
        
        let ports = midi_input.ports();
        if ports.is_empty() {
            return Err("Не найдены MIDI входные порты".into());
        }
        
        // Находим порт по ID
        let mut selected_port = None;
        for port in ports.iter() {
            if let Ok(port_name) = midi_input.port_name(port) {
                if port_name == self.port_info.name {
                    selected_port = Some(port.clone());
                    break;
                }
            }
        }
        
        let port = selected_port.ok_or("Порт не найден")?;
        
        // Создаем подключение с callback
        let connection = midi_input.connect(
            &port,
            "midi-curves-input",
            move |_timestamp, data, _| {
                // В реальной реализации здесь был бы callback, но пока оставим простой вывод
                if !data.is_empty() {
                    println!("🎹 MIDI данные получены: {:?}", data);
                }
            },
            callback,
        )?;
        
        self.connection = Some(connection);
        println!("🔗 Callback установлен для входного порта: {}", self.port_info.name);
        
        Ok(())
    }
    
    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }
    
    pub fn disconnect(&mut self) {
        if let Some(connection) = self.connection.take() {
            drop(connection); // Закрываем подключение
            println!("🔌 Отключен входной порт: {}", self.port_info.name);
        }
    }
    
    pub fn get_info(&self) -> &MidiPortInfo {
        &self.port_info
    }
}

impl Drop for MidiInputPort {
    fn drop(&mut self) {
        self.disconnect();
    }
}

// Выходной MIDI порт
pub struct MidiOutputPort {
    port_info: MidiPortInfo,
    connection: Option<MidiOutputConnection>,
}

impl MidiOutputPort {
    pub fn new(port_name: &str) -> Result<Self, Box<dyn Error>> {
        // Получаем доступные MIDI выходные порты
        let midi_output = MidiOutput::new("MIDI Curves Output")?;
        
        let ports = midi_output.ports();
        if ports.is_empty() {
            return Err("Не найдены MIDI выходные порты".into());
        }
        
        // Ищем порт по имени
        let mut selected_port = None;
        for (i, port) in ports.iter().enumerate() {
            if let Ok(port_name_str) = midi_output.port_name(port) {
                if port_name_str.contains(port_name) {
                    selected_port = Some((i, port.clone()));
                    break;
                }
            }
        }
        
        // Если конкретный порт не найден, используем первый доступный
        let (port_index, port) = match selected_port {
            Some((index, port)) => (index, port),
            None => {
                // Показываем все доступные порты
                println!("🎵 Доступные MIDI выходные порты:");
                for (i, port) in ports.iter().enumerate() {
                    if let Ok(port_name) = midi_output.port_name(port) {
                        println!("  {}: {}", i, port_name);
                    }
                }
                
                if ports.is_empty() {
                    return Err("MIDI выходные порты не найдены".into());
                }
                
                (0, ports[0].clone())
            }
        };
        
        // Получаем имя выбранного порта
        let port_name = midi_output.port_name(&port)
            .unwrap_or_else(|_| format!("Порт {}", port_index));
        
        let port_info = MidiPortInfo::new(
            format!("output_{}", port_index),
            port_name,
            false,
            true,
        );
        
        println!("✅ Создан выходной MIDI порт: {}", port_info.name);
        
        // Создаем подключение
        let connection = midi_output.connect(&port, "midi-curves-output")?;
        
        Ok(Self {
            port_info,
            connection: Some(connection),
        })
    }
    
    pub fn send_midi(&mut self, data: &[u8]) -> Result<(), Box<dyn Error>> {
        if let Some(connection) = &mut self.connection {
            connection.send(data)?;
            Ok(())
        } else {
            Err("Выходной порт не подключен".into())
        }
    }
    
    pub fn send_note_on(&mut self, channel: u8, note: u8, velocity: u8) -> Result<(), Box<dyn Error>> {
        let status = 0x90 | (channel & 0x0F);
        self.send_midi(&[status, note, velocity])
    }
    
    pub fn send_note_off(&mut self, channel: u8, note: u8, velocity: u8) -> Result<(), Box<dyn Error>> {
        let status = 0x80 | (channel & 0x0F);
        self.send_midi(&[status, note, velocity])
    }
    
    pub fn send_control_change(&mut self, channel: u8, controller: u8, value: u8) -> Result<(), Box<dyn Error>> {
        let status = 0xB0 | (channel & 0x0F);
        self.send_midi(&[status, controller, value])
    }
    
    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }
    
    pub fn disconnect(&mut self) {
        if let Some(connection) = self.connection.take() {
            drop(connection); // Закрываем подключение
            println!("🔌 Отключен выходной порт: {}", self.port_info.name);
        }
    }
    
    pub fn get_info(&self) -> &MidiPortInfo {
        &self.port_info
    }
}

impl Drop for MidiOutputPort {
    fn drop(&mut self) {
        self.disconnect();
    }
}

// Менеджер MIDI портов
pub struct MidiPortManager {
    pub input_ports: HashMap<String, MidiInputPort>,
    pub output_ports: HashMap<String, MidiOutputPort>,
}

impl MidiPortManager {
    pub fn new() -> Self {
        Self {
            input_ports: HashMap::new(),
            output_ports: HashMap::new(),
        }
    }
    
    /// Установка входного порта
    pub fn set_input_port(&mut self, port: MidiInputPort) {
        let port_id = port.get_info().id.clone();
        self.input_ports.insert(port_id, port);
    }
    
    /// Получение входного порта
    pub fn get_input_port(&self, port_id: &str) -> Option<&MidiInputPort> {
        self.input_ports.get(port_id)
    }
    
    /// Установка выходного порта
    pub fn set_output_port(&mut self, port: MidiOutputPort) {
        let port_id = port.get_info().id.clone();
        self.output_ports.insert(port_id, port);
    }
    
    /// Получение выходного порта
    pub fn get_output_port(&self, port_id: &str) -> Option<&MidiOutputPort> {
        self.output_ports.get(port_id)
    }
    
    /// Получение выходного порта для изменения
    pub fn get_output_port_mut(&mut self, port_id: &str) -> Option<&mut MidiOutputPort> {
        self.output_ports.get_mut(port_id)
    }
    
    /// Отключение всех портов
    pub fn disconnect_all(&mut self) -> Result<(), Box<dyn Error>> {
        // Отключаем все входные порты
        for port in self.input_ports.values_mut() {
            port.disconnect();
        }
        
        // Отключаем все выходные порты
        for port in self.output_ports.values_mut() {
            port.disconnect();
        }
        
        // Очищаем коллекции
        self.input_ports.clear();
        self.output_ports.clear();
        
        Ok(())
    }
    
    /// Проверка наличия активных подключений
    pub fn has_active_connections(&self) -> bool {
        !self.input_ports.is_empty() || !self.output_ports.is_empty()
    }
    
    /// Получение списка активных входных портов
    pub fn get_active_input_ports(&self) -> Vec<&MidiInputPort> {
        self.input_ports.values().filter(|p| p.is_connected()).collect()
    }
    
    /// Получение списка активных выходных портов
    pub fn get_active_output_ports(&self) -> Vec<&MidiOutputPort> {
        self.output_ports.values().filter(|p| p.is_connected()).collect()
    }
    
    /// Отправка MIDI сообщения на все активные выходные порты
    pub fn broadcast_midi(&mut self, data: &[u8]) -> Result<(), Box<dyn Error>> {
        let mut errors = Vec::new();
        
        for port in self.output_ports.values_mut() {
            if port.is_connected() {
                if let Err(e) = port.send_midi(data) {
                    errors.push(format!("Ошибка отправки на {}: {}", port.get_info().name, e));
                }
            }
        }
        
        if !errors.is_empty() {
            return Err(format!("Ошибки отправки MIDI: {}", errors.join("; ")).into());
        }
        
        Ok(())
    }
    
    /// Отправка NoteOn на все активные выходные порты
    pub fn broadcast_note_on(&mut self, channel: u8, note: u8, velocity: u8) -> Result<(), Box<dyn Error>> {
        let status = 0x90 | (channel & 0x0F);
        self.broadcast_midi(&[status, note, velocity])
    }
    
    /// Отправка NoteOff на все активные выходные порты
    pub fn broadcast_note_off(&mut self, channel: u8, note: u8, velocity: u8) -> Result<(), Box<dyn Error>> {
        let status = 0x80 | (channel & 0x0F);
        self.broadcast_midi(&[status, note, velocity])
    }
}

// Создание виртуальных MIDI портов для тестирования
#[cfg(feature = "virtual_ports")]
pub fn create_virtual_ports() -> Result<(MidiInputPort, MidiOutputPort), Box<dyn Error>> {
    // В реальной реализации здесь был бы код создания виртуальных портов
    // Например, через JACK MIDI на Linux или CoreMIDI виртуальные порты на macOS
    unimplemented!("Создание виртуальных MIDI портов еще не реализовано")
}

// Утилиты для работы с MIDI портами
pub mod utils {
    use super::*;
    
    /// Поиск портов по шаблону имени
    pub fn find_ports_by_pattern(
        pattern: &str, 
        is_input: bool, 
        is_output: bool
    ) -> Result<Vec<MidiPortInfo>, Box<dyn Error>> {
        let mut ports = Vec::new();
        
        if is_input {
            let midi_input = MidiInput::new("Port Finder")?;
            for (i, port) in midi_input.ports().iter().enumerate() {
                if let Ok(port_name) = midi_input.port_name(port) {
                    if port_name.contains(pattern) {
                        ports.push(MidiPortInfo::new(
                            format!("input_{}", i),
                            port_name,
                            true,
                            false,
                        ));
                    }
                }
            }
        }
        
        if is_output {
            let midi_output = MidiOutput::new("Port Finder")?;
            for (i, port) in midi_output.ports().iter().enumerate() {
                if let Ok(port_name) = midi_output.port_name(port) {
                    if port_name.contains(pattern) {
                        ports.push(MidiPortInfo::new(
                            format!("output_{}", i),
                            port_name,
                            false,
                            true,
                        ));
                    }
                }
            }
        }
        
        Ok(ports)
    }
    
    /// Получение всех доступных MIDI портов
    pub fn get_all_available_ports() -> Result<Vec<MidiPortInfo>, Box<dyn Error>> {
        let mut ports = Vec::new();
        
        // Входные порты
        let midi_input = MidiInput::new("Port Scanner")?;
        for (i, port) in midi_input.ports().iter().enumerate() {
            if let Ok(port_name) = midi_input.port_name(port) {
                ports.push(MidiPortInfo::new(
                    format!("input_{}", i),
                    port_name,
                    true,
                    false,
                ));
            }
        }
        
        // Выходные порты
        let midi_output = MidiOutput::new("Port Scanner")?;
        for (i, port) in midi_output.ports().iter().enumerate() {
            if let Ok(port_name) = midi_output.port_name(port) {
                ports.push(MidiPortInfo::new(
                    format!("output_{}", i),
                    port_name,
                    false,
                    true,
                ));
            }
        }
        
        Ok(ports)
    }
}