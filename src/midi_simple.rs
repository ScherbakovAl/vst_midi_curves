//! Simplified MIDI support for basic testing
//!
//! Copyright (c) 2025 Scherbakov Alexey <scherbakov.al@gmail.com>
//! Licensed under the MIT License
//!
//! This is a temporary simplified version for checking basic MIDI functionality
//! without complex dependencies and error handling.
//!
//! Supports separate curves for NoteOn and NoteOff events.

use std::sync::{Arc, Mutex};

// Simple MIDI events
#[derive(Debug, Clone, PartialEq)]
pub enum SimpleMidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8, velocity: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    Other,
}

impl SimpleMidiEvent {
// Get channel from event
    pub fn get_channel(&self) -> u8 {
        match self {
            SimpleMidiEvent::NoteOn { channel, .. } => *channel,
            SimpleMidiEvent::NoteOff { channel, .. } => *channel,
            SimpleMidiEvent::ControlChange { channel, .. } => *channel,
            SimpleMidiEvent::Other => 0,
        }
    }
    
// Get note from event
    pub fn get_note(&self) -> u8 {
        match self {
            SimpleMidiEvent::NoteOn { note, .. } => *note,
            SimpleMidiEvent::NoteOff { note, .. } => *note,
            SimpleMidiEvent::ControlChange { controller, .. } => *controller,
            SimpleMidiEvent::Other => 60, // Middle C by default
        }
    }
}

// Simple MIDI stats
#[derive(Debug, Clone, Default)]
pub struct SimpleMidiStats {
    pub note_on_count: u64,
    pub note_off_count: u64,
    pub control_change_count: u64,
}

// Simple MIDI manager
pub struct SimpleMidiManager {
    dual_curve_processor: Arc<Mutex<crate::curve::DualCurve>>,
    stats: SimpleMidiStats,
    input_ports: Vec<String>,
    output_ports: Vec<String>,
    is_active: bool,
    
    // Callback for MIDI event notifications
    event_callback: Option<Arc<dyn Fn(&SimpleMidiEvent) + Send + Sync>>,
}

impl SimpleMidiManager {
    pub fn new(dual_curve_processor: Arc<Mutex<crate::curve::DualCurve>>) -> Self {
        Self {
            dual_curve_processor,
            stats: SimpleMidiStats::default(),
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            is_active: false,
            event_callback: None,
        }
    }
    
    // Set callback for MIDI events
    pub fn set_event_callback(&mut self, callback: Arc<dyn Fn(&SimpleMidiEvent) + Send + Sync>) {
        self.event_callback = Some(callback);
    }
    
    // Call callback if it is set
    fn notify_callback(&self, event: &SimpleMidiEvent) {
        if let Some(ref callback) = self.event_callback {
            callback(event);
        }
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
    
    pub fn is_active(&self) -> bool {
        self.is_active
    }
    
pub fn process_midi_event(&mut self, event: &SimpleMidiEvent) -> Option<SimpleMidiEvent> {
        let result = match event {
            SimpleMidiEvent::NoteOn { channel, note, velocity } => {
                // Apply NoteOn curve to velocity
                let mut dual_curve = self.dual_curve_processor.lock().unwrap();
                let processed_velocity = dual_curve.process_note_on_velocity(*velocity);
                self.stats.note_on_count += 1;
                
                let processed_event = SimpleMidiEvent::NoteOn {
                    channel: *channel,
                    note: *note,
                    velocity: processed_velocity,
                };
                
                // Notify about incoming event
                self.notify_callback(event);
                // Notify about processed event
                self.notify_callback(&processed_event);
                
                Some(processed_event)
            }
            SimpleMidiEvent::NoteOff { channel, note, velocity } => {
                // Apply NoteOff curve to velocity
                let mut dual_curve = self.dual_curve_processor.lock().unwrap();
                let processed_velocity = dual_curve.process_note_off_velocity(*velocity);
                self.stats.note_off_count += 1;
                
                let processed_event = SimpleMidiEvent::NoteOff {
                    channel: *channel,
                    note: *note,
                    velocity: processed_velocity,
                };
                
                // Notify about incoming event
                self.notify_callback(event);
                // Notify about processed event
                self.notify_callback(&processed_event);
                
                Some(processed_event)
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
        // Add dummy ports for testing
        self.input_ports = vec![
            "Test Input 1".to_string(),
            "MIDI Keyboard".to_string(),
        ];
        self.output_ports = vec![
            "Test Output 1".to_string(),
            "Virtual Synth".to_string(),
        ];
    }
    
// Generate test MIDI event
    pub fn generate_test_event(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🎵 Testing both curves:");
        
        // Test NoteOn event
        let note_on_event = SimpleMidiEvent::NoteOn {
            channel: 0,
            note: 60, // Middle C
            velocity: 64, // Medium velocity
        };
        
        println!("  NoteOn event: {:?}", note_on_event);
        if let Some(processed_note_on) = self.process_midi_event(&note_on_event) {
            println!("  → Processed NoteOn: {:?}", processed_note_on);
        }
        
        // Test NoteOff event
        let note_off_event = SimpleMidiEvent::NoteOff {
            channel: 0,
            note: 60,
            velocity: 64,
        };
        
        println!("  NoteOff event: {:?}", note_off_event);
        if let Some(processed_note_off) = self.process_midi_event(&note_off_event) {
            println!("  → Processed NoteOff: {:?}", processed_note_off);
        }
        
        Ok(())
    }
}