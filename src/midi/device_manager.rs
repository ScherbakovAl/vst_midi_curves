//! MIDI device management and discovery
//!
//! This module is responsible for:
//! - Automatic MIDI device discovery on startup
//! - Tracking device connection/disconnection
//! - Updating available port lists
//! - Monitoring device status

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use midir::{MidiInput, MidiOutput};
use super::ports::MidiPortInfo;

// MIDI device information
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
    
    /// Determine virtual device by name
    fn is_virtual_device(name: &str) -> bool {
        let name_lower = name.to_lowercase();
        name_lower.contains("virtual") ||
        name_lower.contains("iac") ||
        name_lower.contains("loop") ||
        name_lower.contains("vir") ||
        name_lower.contains("virtual midi")
    }
    
    /// Update last seen time
    pub fn update_seen(&mut self) {
        self.last_seen = Instant::now();
        self.is_connected = true;
        self.connection_count += 1;
    }
    
    /// Check device validity
    pub fn is_current(&self, timeout: Duration) -> bool {
        self.last_seen.elapsed() < timeout && self.is_connected
    }
}

// Device Manager configuration
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
            scan_interval: Duration::from_millis(500), // Scan every 500ms
            device_timeout: Duration::from_secs(10),   // Device considered disconnected after 10 sec
            auto_reconnect: true,
            prefer_virtual_devices: false,
            enable_hot_plug: true,
        }
    }
}

// Callback for device change notifications
pub type DeviceChangeCallback = Arc<dyn Fn(DeviceChange) + Send + Sync>;

#[derive(Debug, Clone)]
pub enum DeviceChange {
    DeviceConnected(MidiDevice),
    DeviceDisconnected(MidiDevice),
    DeviceListUpdated,
    ScanCompleted,
}

// MIDI device manager
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
    
    /// Start device scanning
    pub fn start_scanning(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_scanning {
            return Ok(());
        }
        
        self.is_scanning = true;
        
        // Scan devices immediately
        self.scan_ports()?;
        
        // Start background thread for periodic scanning
        if self.config.enable_hot_plug {
            let config = self.config.clone();
            let devices = Arc::new(Mutex::new(HashMap::new()));
            let callbacks = self.change_callbacks.clone();
            
            // Copy current devices
            {
                let mut device_map = devices.lock().unwrap();
                for (id, device) in &self.devices {
                    device_map.insert(id.clone(), device.clone());
                }
            }
            
            self.scan_thread = Some(thread::spawn(move || {
                loop {
                    thread::sleep(config.scan_interval);
                    
                    // Scan ports
                    let (input_devices, output_devices) = Self::scan_all_ports();
                    
                    let mut device_map = devices.lock().unwrap();
                    let mut changes = Vec::new();
                    
                    // Update input devices
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
                    
                    // Update output devices
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
                    
                    // Check disconnected devices
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
                    
                    // Notify about changes
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
        
        println!("🔍 MIDI device scanning started");
        Ok(())
    }
    
    /// Stop scanning
    pub fn stop_scanning(&mut self) {
        self.is_scanning = false;
        
        if let Some(thread) = self.scan_thread.take() {
            thread.join().ok();
        }
        
        println!("🛑 MIDI device scanning stopped");
    }
    
    /// Scan all available ports
    pub fn scan_ports(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let (input_devices, output_devices) = Self::scan_all_ports();
        
        let mut connected_devices = HashMap::new();
        
        // Process input devices
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
        
        // Process output devices
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
        
        // Update device list
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
        
        // Find disconnected devices
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
        
        println!("📊 Found {} new devices, disconnected {}", connected_count, disconnected_count);
        println!("📋 Total available devices: {}", self.devices.len());
        
        // Notify about changes
        for callback in &self.change_callbacks {
            callback(DeviceChange::ScanCompleted);
        }
        
        Ok(())
    }
    
    /// Actual port scanning
    fn scan_all_ports() -> (Vec<MidiPortInfo>, Vec<MidiPortInfo>) {
        let mut input_devices = Vec::new();
        let mut output_devices = Vec::new();
        
        // Scan input ports
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
                eprintln!("Input port scan error: {}", e);
            }
        }
        
        // Scan output ports
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
                eprintln!("Output port scan error: {}", e);
            }
        }
        
        (input_devices, output_devices)
    }
    
    /// Add callback for device changes
    pub fn add_device_change_callback(&mut self, callback: DeviceChangeCallback) {
        self.change_callbacks.push(callback);
    }
    
    /// Remove callback for device changes
    pub fn remove_device_change_callback(&mut self, callback: &DeviceChangeCallback) {
        self.change_callbacks.retain(|cb| Arc::ptr_eq(cb, callback));
    }
    
    /// Get list of all available devices
    pub fn get_all_devices(&self) -> Vec<&MidiDevice> {
        self.devices.values().filter(|d| d.is_current(self.config.device_timeout)).collect()
    }
    
    /// Get list of input devices
    pub fn get_input_devices(&self) -> Vec<&MidiDevice> {
        self.devices.values()
            .filter(|d| d.is_input)
            .collect()
    }
    
    /// Get list of output devices
    pub fn get_output_devices(&self) -> Vec<&MidiDevice> {
        self.devices.values()
            .filter(|d| d.is_output)
            .collect()
    }
    
    /// Get input port names
    pub fn get_input_port_names(&self) -> Vec<String> {
        self.devices.values()
            .filter(|d| d.is_input)
            .filter(|d| d.is_connected || d.last_seen.elapsed() < Duration::from_secs(30)) // Show devices seen in last 30 seconds
            .map(|d| d.name.clone())
            .collect()
    }
    
    /// Get output port names
    pub fn get_output_port_names(&self) -> Vec<String> {
        self.devices.values()
            .filter(|d| d.is_output)
            .filter(|d| d.is_connected || d.last_seen.elapsed() < Duration::from_secs(30)) // Show devices seen in last 30 seconds
            .map(|d| d.name.clone())
            .collect()
    }
    
    /// Get device by name
    pub fn get_device_by_name(&self, name: &str) -> Option<&MidiDevice> {
        self.devices.values().find(|d| d.name == name)
    }
    
    /// Get virtual devices
    pub fn get_virtual_devices(&self) -> Vec<&MidiDevice> {
        self.devices.values()
            .filter(|d| d.is_virtual && d.is_current(self.config.device_timeout))
            .collect()
    }
    
    /// Get physical devices
    pub fn get_physical_devices(&self) -> Vec<&MidiDevice> {
        self.devices.values()
            .filter(|d| !d.is_virtual && d.is_current(self.config.device_timeout))
            .collect()
    }
    
    /// Check if devices exist
    pub fn has_devices(&self) -> bool {
        !self.get_all_devices().is_empty()
    }
    
    /// Check if input devices exist
    pub fn has_input_devices(&self) -> bool {
        !self.get_input_devices().is_empty()
    }
    
    /// Check if output devices exist
    pub fn has_output_devices(&self) -> bool {
        !self.get_output_devices().is_empty()
    }
    
    /// Get last scan time
    pub fn get_last_scan_time(&self) -> Option<Duration> {
        self.last_scan_time.map(|time| time.elapsed())
    }
    
    /// Get configuration
    pub fn get_config(&self) -> &DeviceManagerConfig {
        &self.config
    }
    
    /// Set configuration
    pub fn set_config(&mut self, config: DeviceManagerConfig) {
        self.config = config;
    }
    
    /// Clear all devices
    pub fn clear_devices(&mut self) {
        self.devices.clear();
        println!("🗑️ MIDI device list cleared");
        
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

// Utilities for Device Manager
pub mod utils {
    use super::*;
    
    /// Create Device Manager with development settings
    pub fn dev_manager_config() -> DeviceManagerConfig {
        DeviceManagerConfig {
            scan_interval: Duration::from_millis(200), // Fast scanning for development
            device_timeout: Duration::from_secs(5),    // Fast disconnection for testing
            auto_reconnect: true,
            prefer_virtual_devices: true, // Prefer virtual for development
            enable_hot_plug: true,
        }
    }
    
    /// Create Device Manager with production settings
    pub fn production_manager_config() -> DeviceManagerConfig {
        DeviceManagerConfig {
            scan_interval: Duration::from_millis(1000), // Slow scanning
            device_timeout: Duration::from_secs(30),    // Long timeout
            auto_reconnect: true,
            prefer_virtual_devices: false,
            enable_hot_plug: true,
        }
    }
    
    /// Check MIDI system availability
    pub fn check_midi_system() -> Result<(Vec<String>, Vec<String>), Box<dyn std::error::Error>> {
        let mut input_devices = Vec::new();
        let mut output_devices = Vec::new();
        
        // Check input ports
        match MidiInput::new("System Check") {
            Ok(midi_input) => {
                for port in midi_input.ports() {
                    if let Ok(port_name) = midi_input.port_name(&port) {
                        input_devices.push(port_name);
                    }
                }
            }
            Err(e) => return Err(format!("MIDI Input unavailable: {}", e).into()),
        }
        
        // Check output ports
        match MidiOutput::new("System Check") {
            Ok(midi_output) => {
                for port in midi_output.ports() {
                    if let Ok(port_name) = midi_output.port_name(&port) {
                        output_devices.push(port_name);
                    }
                }
            }
            Err(e) => return Err(format!("MIDI Output unavailable: {}", e).into()),
        }
        
        Ok((input_devices, output_devices))
    }
}