//! Управление и обнаружение MIDI устройств
//! 
//! Этот модуль отвечает за:
//! - Автоматическое обнаружение MIDI устройств при запуске
//! - Отслеживание подключения/отключения устройств
//! - Обновление списков доступных портов
//! - Мониторинг состояния устройств

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use midir::{MidiInput, MidiOutput};
use super::ports::{MidiPortInfo, utils as port_utils};

// Информация о MIDI устройстве
#[derive(Debug, Clone)]
pub struct MidiDevice {
    pub id: String,
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
    pub is_virtual: bool,
    pub is_connected: bool,
    pub last_seen: Instant,
    pub connection_count: u32,
}

impl MidiDevice {
    pub fn new(id: String, name: String, is_input: bool, is_output: bool) -> Self {
        let is_virtual = Self::is_virtual_device(&name);
        Self {
            id,
            name,
            is_input,
            is_output,
            is_virtual,
            is_connected: false,
            last_seen: Instant::now(),
            connection_count: 0,
        }
    }
    
    /// Определение виртуального устройства по имени
    fn is_virtual_device(name: &str) -> bool {
        let name_lower = name.to_lowercase();
        name_lower.contains("virtual") || 
        name_lower.contains("iac") || 
        name_lower.contains("loop") ||
        name_lower.contains("vir") ||
        name_lower.contains("virtual midi")
    }
    
    /// Обновление времени последнего обнаружения
    pub fn update_seen(&mut self) {
        self.last_seen = Instant::now();
        self.is_connected = true;
        self.connection_count += 1;
    }
    
    /// Проверка актуальности устройства
    pub fn is_current(&self, timeout: Duration) -> bool {
        self.last_seen.elapsed() < timeout && self.is_connected
    }
}

// Конфигурация Device Manager
#[derive(Debug, Clone)]
pub struct DeviceManagerConfig {
    pub scan_interval: Duration,
    pub device_timeout: Duration,
    pub auto_reconnect: bool,
    pub prefer_virtual_devices: bool,
    pub enable_hot_plug: bool,
}

impl Default for DeviceManagerConfig {
    fn default() -> Self {
        Self {
            scan_interval: Duration::from_millis(500), // Сканирование каждые 500мс
            device_timeout: Duration::from_secs(10),   // Устройство считается отключенным через 10 сек
            auto_reconnect: true,
            prefer_virtual_devices: false,
            enable_hot_plug: true,
        }
    }
}

// Callback для уведомлений об изменениях устройств
pub type DeviceChangeCallback = Arc<dyn Fn(DeviceChange) + Send + Sync>;

#[derive(Debug, Clone)]
pub enum DeviceChange {
    DeviceConnected(MidiDevice),
    DeviceDisconnected(MidiDevice),
    DeviceListUpdated,
    ScanCompleted,
}

// Менеджер MIDI устройств
pub struct DeviceManager {
    devices: HashMap<String, MidiDevice>,
    config: DeviceManagerConfig,
    scan_thread: Option<thread::JoinHandle<()>>,
    is_scanning: bool,
    change_callbacks: Vec<DeviceChangeCallback>,
    last_scan_time: Option<Instant>,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            devices: HashMap::new(),
            config: DeviceManagerConfig::default(),
            scan_thread: None,
            is_scanning: false,
            change_callbacks: Vec::new(),
            last_scan_time: None,
        }
    }
    
    pub fn new_with_config(config: DeviceManagerConfig) -> Self {
        Self {
            devices: HashMap::new(),
            config,
            scan_thread: None,
            is_scanning: false,
            change_callbacks: Vec::new(),
            last_scan_time: None,
        }
    }
    
    /// Запуск сканирования устройств
    pub fn start_scanning(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_scanning {
            return Ok(());
        }
        
        self.is_scanning = true;
        
        // Сканируем устройства сразу
        self.scan_ports()?;
        
        // Запускаем фоновый поток для периодического сканирования
        if self.config.enable_hot_plug {
            let config = self.config.clone();
            let devices = Arc::new(Mutex::new(HashMap::new()));
            let callbacks = self.change_callbacks.clone();
            
            // Копируем текущие устройства
            {
                let mut device_map = devices.lock().unwrap();
                for (id, device) in &self.devices {
                    device_map.insert(id.clone(), device.clone());
                }
            }
            
            self.scan_thread = Some(thread::spawn(move || {
                loop {
                    thread::sleep(config.scan_interval);
                    
                    // Сканируем порты
                    let (input_devices, output_devices) = Self::scan_all_ports();
                    
                    let mut device_map = devices.lock().unwrap();
                    let mut changes = Vec::new();
                    
                    // Обновляем входные устройства
                    for device_info in input_devices {
                        let device_id = format!("input_{}", device_info.name.replace(" ", "_"));
                        if let Some(existing_device) = device_map.get_mut(&device_id) {
                            existing_device.update_seen();
                        } else {
                            let new_device = MidiDevice::new(
                                device_id.clone(),
                                device_info.name.clone(),
                                true,
                                false,
                            );
                            device_map.insert(device_id.clone(), new_device.clone());
                            changes.push(DeviceChange::DeviceConnected(new_device));
                        }
                    }
                    
                    // Обновляем выходные устройства
                    for device_info in output_devices {
                        let device_id = format!("output_{}", device_info.name.replace(" ", "_"));
                        if let Some(existing_device) = device_map.get_mut(&device_id) {
                            existing_device.update_seen();
                        } else {
                            let new_device = MidiDevice::new(
                                device_id.clone(),
                                device_info.name.clone(),
                                false,
                                true,
                            );
                            device_map.insert(device_id.clone(), new_device.clone());
                            changes.push(DeviceChange::DeviceConnected(new_device));
                        }
                    }
                    
                    // Проверяем отключенные устройства
                    let mut disconnected = Vec::new();
                    for (id, device) in device_map.iter_mut() {
                        if !device.is_current(config.device_timeout) {
                            disconnected.push(id.clone());
                        }
                    }
                    
                    for id in disconnected {
                        if let Some(disconnected_device) = device_map.remove(&id) {
                            changes.push(DeviceChange::DeviceDisconnected(disconnected_device));
                        }
                    }
                    
                    // Уведомляем о изменениях
                    if !changes.is_empty() {
                        for callback in &callbacks {
                            for change in &changes {
                                callback(change.clone());
                            }
                        }
                    }
                }
            }));
        }
        
        println!("🔍 Сканирование MIDI устройств запущено");
        Ok(())
    }
    
    /// Остановка сканирования
    pub fn stop_scanning(&mut self) {
        self.is_scanning = false;
        
        if let Some(thread) = self.scan_thread.take() {
            thread.join().ok();
        }
        
        println!("🛑 Сканирование MIDI устройств остановлено");
    }
    
    /// Сканирование всех доступных портов
    pub fn scan_ports(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let (input_devices, output_devices) = Self::scan_all_ports();
        
        let mut connected_devices = HashMap::new();
        
        // Обработка входных устройств
        for device_info in input_devices {
            let device_id = format!("input_{}", device_info.name.replace(" ", "_"));
            let device = MidiDevice::new(
                device_id.clone(),
                device_info.name.clone(),
                true,
                false,
            );
            connected_devices.insert(device_id, device);
        }
        
        // Обработка выходных устройств
        for device_info in output_devices {
            let device_id = format!("output_{}", device_info.name.replace(" ", "_"));
            let device = MidiDevice::new(
                device_id.clone(),
                device_info.name.clone(),
                false,
                true,
            );
            connected_devices.insert(device_id, device);
        }
        
        // Обновляем список устройств
        let mut connected_count = 0;
        let mut disconnected_count = 0;
        
        for (id, device) in connected_devices.iter() {
            if let Some(existing_device) = self.devices.get_mut(id) {
                existing_device.update_seen();
            } else {
                self.devices.insert(id.clone(), device.clone());
                connected_count += 1;
            }
        }
        
        // Находим отключенные устройства
        let mut to_remove = Vec::new();
        for (id, device) in &self.devices {
            if !connected_devices.contains_key(id) && device.is_current(self.config.device_timeout) {
                to_remove.push(id.clone());
            }
        }
        
        for id in to_remove {
            disconnected_count += 1;
            self.devices.remove(&id);
        }
        
        self.last_scan_time = Some(Instant::now());
        
        println!("📊 Найдено {} новых устройств, отключено {}", connected_count, disconnected_count);
        println!("📋 Всего доступных устройств: {}", self.devices.len());
        
        // Уведомляем об изменениях
        for callback in &self.change_callbacks {
            callback(DeviceChange::ScanCompleted);
        }
        
        Ok(())
    }
    
    /// Фактическое сканирование портов
    fn scan_all_ports() -> (Vec<MidiPortInfo>, Vec<MidiPortInfo>) {
        let mut input_devices = Vec::new();
        let mut output_devices = Vec::new();
        
        // Сканирование входных портов
        match MidiInput::new("Device Scanner") {
            Ok(midi_input) => {
                for (i, port) in midi_input.ports().iter().enumerate() {
                    if let Ok(port_name) = midi_input.port_name(port) {
                        let device_info = MidiPortInfo::new(
                            format!("input_{}", i),
                            port_name,
                            true,
                            false,
                        );
                        input_devices.push(device_info);
                    }
                }
            }
            Err(e) => {
                eprintln!("Ошибка сканирования входных портов: {}", e);
            }
        }
        
        // Сканирование выходных портов
        match MidiOutput::new("Device Scanner") {
            Ok(midi_output) => {
                for (i, port) in midi_output.ports().iter().enumerate() {
                    if let Ok(port_name) = midi_output.port_name(port) {
                        let device_info = MidiPortInfo::new(
                            format!("output_{}", i),
                            port_name,
                            false,
                            true,
                        );
                        output_devices.push(device_info);
                    }
                }
            }
            Err(e) => {
                eprintln!("Ошибка сканирования выходных портов: {}", e);
            }
        }
        
        (input_devices, output_devices)
    }
    
    /// Добавление callback для изменений устройств
    pub fn add_device_change_callback(&mut self, callback: DeviceChangeCallback) {
        self.change_callbacks.push(callback);
    }
    
    /// Удаление callback для изменений устройств
    pub fn remove_device_change_callback(&mut self, callback: &DeviceChangeCallback) {
        self.change_callbacks.retain(|cb| Arc::ptr_eq(cb, callback));
    }
    
    /// Получение списка всех доступных устройств
    pub fn get_all_devices(&self) -> Vec<&MidiDevice> {
        self.devices.values().filter(|d| d.is_current(self.config.device_timeout)).collect()
    }
    
    /// Получение списка входных устройств
    pub fn get_input_devices(&self) -> Vec<&MidiDevice> {
        self.devices.values()
            .filter(|d| d.is_input)
            .collect()
    }
    
    /// Получение списка выходных устройств
    pub fn get_output_devices(&self) -> Vec<&MidiDevice> {
        self.devices.values()
            .filter(|d| d.is_output)
            .collect()
    }
    
    /// Получение имен входных портов
    pub fn get_input_port_names(&self) -> Vec<String> {
        self.devices.values()
            .filter(|d| d.is_input)
            .filter(|d| d.is_connected || d.last_seen.elapsed() < Duration::from_secs(30)) // Показываем устройства, которые были видны в последние 30 секунд
            .map(|d| d.name.clone())
            .collect()
    }
    
    /// Получение имен выходных портов
    pub fn get_output_port_names(&self) -> Vec<String> {
        self.devices.values()
            .filter(|d| d.is_output)
            .filter(|d| d.is_connected || d.last_seen.elapsed() < Duration::from_secs(30)) // Показываем устройства, которые были видны в последние 30 секунд
            .map(|d| d.name.clone())
            .collect()
    }
    
    /// Получение устройства по имени
    pub fn get_device_by_name(&self, name: &str) -> Option<&MidiDevice> {
        self.devices.values().find(|d| d.name == name)
    }
    
    /// Получение виртуальных устройств
    pub fn get_virtual_devices(&self) -> Vec<&MidiDevice> {
        self.devices.values()
            .filter(|d| d.is_virtual && d.is_current(self.config.device_timeout))
            .collect()
    }
    
    /// Получение физических устройств
    pub fn get_physical_devices(&self) -> Vec<&MidiDevice> {
        self.devices.values()
            .filter(|d| !d.is_virtual && d.is_current(self.config.device_timeout))
            .collect()
    }
    
    /// Проверка наличия устройств
    pub fn has_devices(&self) -> bool {
        !self.get_all_devices().is_empty()
    }
    
    /// Проверка наличия входных устройств
    pub fn has_input_devices(&self) -> bool {
        !self.get_input_devices().is_empty()
    }
    
    /// Проверка наличия выходных устройств
    pub fn has_output_devices(&self) -> bool {
        !self.get_output_devices().is_empty()
    }
    
    /// Получение времени последнего сканирования
    pub fn get_last_scan_time(&self) -> Option<Duration> {
        self.last_scan_time.map(|time| time.elapsed())
    }
    
    /// Получение конфигурации
    pub fn get_config(&self) -> &DeviceManagerConfig {
        &self.config
    }
    
    /// Установка конфигурации
    pub fn set_config(&mut self, config: DeviceManagerConfig) {
        self.config = config;
    }
    
    /// Очистка всех устройств
    pub fn clear_devices(&mut self) {
        self.devices.clear();
        println!("🗑️ Очищен список MIDI устройств");
        
        for callback in &self.change_callbacks {
            callback(DeviceChange::DeviceListUpdated);
        }
    }
}

impl Drop for DeviceManager {
    fn drop(&mut self) {
        self.stop_scanning();
    }
}

// Утилиты для работы с Device Manager
pub mod utils {
    use super::*;
    
    /// Создание Device Manager с настройками для разработки
    pub fn dev_manager_config() -> DeviceManagerConfig {
        DeviceManagerConfig {
            scan_interval: Duration::from_millis(200), // Быстрое сканирование для разработки
            device_timeout: Duration::from_secs(5),    // Быстрое отключение для тестирования
            auto_reconnect: true,
            prefer_virtual_devices: true, // Предпочитаем виртуальные для разработки
            enable_hot_plug: true,
        }
    }
    
    /// Создание Device Manager с настройками для продакшена
    pub fn production_manager_config() -> DeviceManagerConfig {
        DeviceManagerConfig {
            scan_interval: Duration::from_millis(1000), // Медленное сканирование
            device_timeout: Duration::from_secs(30),    // Долгое ожидание
            auto_reconnect: true,
            prefer_virtual_devices: false,
            enable_hot_plug: true,
        }
    }
    
    /// Проверка доступности MIDI системы
    pub fn check_midi_system() -> Result<(Vec<String>, Vec<String>), Box<dyn std::error::Error>> {
        let mut input_devices = Vec::new();
        let mut output_devices = Vec::new();
        
        // Проверка входных портов
        match MidiInput::new("System Check") {
            Ok(midi_input) => {
                for port in midi_input.ports() {
                    if let Ok(port_name) = midi_input.port_name(&port) {
                        input_devices.push(port_name);
                    }
                }
            }
            Err(e) => return Err(format!("MIDI Input недоступен: {}", e).into()),
        }
        
        // Проверка выходных портов
        match MidiOutput::new("System Check") {
            Ok(midi_output) => {
                for port in midi_output.ports() {
                    if let Ok(port_name) = midi_output.port_name(&port) {
                        output_devices.push(port_name);
                    }
                }
            }
            Err(e) => return Err(format!("MIDI Output недоступен: {}", e).into()),
        }
        
        Ok((input_devices, output_devices))
    }
}