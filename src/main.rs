//! MIDI Curves - Standalone application for processing MIDI velocity with customizable curves
//!
//! Copyright (c) 2025 Scherbakov Alexey <scherbakov.al@gmail.com>
//! Licensed under the MIT License
//!
//! Now supports separate curves for NoteOn and NoteOff events.
//! Uses DualCurve structure to manage two curves simultaneously.
//! Includes settings save and load system for each platform.

use eframe::egui;
use std::sync::{Arc, Mutex};

// Connect project modules
mod curve;
mod presets;
mod midi;
mod midi_simple;
mod settings;

use curve::DualCurve;
use presets::{PresetManager, CurvePreset};
use midi::{MidiManager, MidiEvent, MidiStats};
use settings::SettingsManager;
use egui::{Pos2, Rect, Sense, Response, Painter, Color32, Stroke};

// MIDI events for GUI display
#[derive(Debug, Clone)]
struct GuiMidiEvent {
    event: MidiEvent,
    timestamp: std::time::Instant,
    is_processed: bool,
}

// Main application structure
struct MidiCurvesApp {
    // Curve processing core - now uses DualCurve for NoteOn and NoteOff
    dual_curve: Arc<Mutex<DualCurve>>,
    
    // MIDI manager for real processing
    midi_manager: Arc<Mutex<MidiManager>>,
    
    midi_input_ports: Vec<String>,
    midi_output_ports: Vec<String>,
    selected_input_port: Option<String>,
    selected_output_port: Option<String>,
    midi_stats: MidiStats,
    
    // Interface state
    selected_point: Option<usize>,
    is_dragging: bool,
    drag_start: Option<Pos2>,
    shift_pressed: bool,
    
    // Active tab: true for NoteOn, false for NoteOff
    active_tab_note_on: bool,
    
    // Preset manager
    preset_manager: Arc<Mutex<PresetManager>>,
    selected_preset: Option<String>,
    
    // Application settings manager
    settings_manager: SettingsManager,
    
    // Save preset dialog state
    show_save_preset_dialog: bool,
    save_preset_name: String,
    save_preset_description: String,
}

impl MidiCurvesApp {
fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Create settings manager (loads saved settings if available)
        let settings_manager = SettingsManager::new()?;
        
        // Create preset manager
        let preset_manager = Arc::new(Mutex::new(PresetManager::new()?));
        
        // Create built-in presets if none exist
        {
            let mut manager = preset_manager.lock().unwrap();
            manager.create_builtin_presets()?;
        }
        
        // Restore DualCurve from settings or create new one
        let dual_curve_processor = Arc::new(Mutex::new(settings_manager.restore_to_dual_curve()));
        
// Initialize application
        let mut app = Self {
            dual_curve: dual_curve_processor.clone(),
            midi_manager: Arc::new(Mutex::new(MidiManager::new(dual_curve_processor.clone()))),
            midi_input_ports: Vec::new(),
            midi_output_ports: Vec::new(),
            selected_input_port: None,
            selected_output_port: None,
            midi_stats: MidiStats::new(),
            
            selected_point: None,
            is_dragging: false,
            drag_start: None,
            shift_pressed: false,
            active_tab_note_on: settings_manager.get_active_curve_tab() == 0, // Restore from settings
            preset_manager,
            selected_preset: None,
            settings_manager,
            show_save_preset_dialog: false,
            save_preset_name: String::new(),
            save_preset_description: String::new(),
        };
        
        // Start full MIDI manager
        {
            let mut midi_manager_mut = app.midi_manager.lock().unwrap();
            midi_manager_mut.start()?;
        }
        
        // Restore last selected MIDI ports
        app.restore_midi_ports_from_settings();
        
        // Restore last preset if available
        app.restore_last_preset_from_settings();
        
        // Update MIDI ports list at startup
        app.refresh_midi_ports();
        
        Ok(app)
    }
    
    /// Restores MIDI ports from saved settings with availability check
    fn restore_midi_ports_from_settings(&mut self) {
        // First update the list of available ports
        self.refresh_midi_ports();
        
        // Save values from settings to variables to avoid borrowing conflicts
        let saved_input_port = self.settings_manager.get_last_input_port().map(|s| s.clone());
        let saved_output_port = self.settings_manager.get_last_output_port().map(|s| s.clone());
        
        // Restore input port
        if let Some(input_port_name) = saved_input_port {
            // Check if the saved port exists in the current list of available ports
            if self.midi_input_ports.iter().any(|port| port == &input_port_name) {
                self.selected_input_port = Some(input_port_name.clone());
                
                // Try to connect to the port
                if let Err(e) = self.connect_input_port(&input_port_name) {
                    eprintln!("Input port restoration error '{}': {}", input_port_name, e);
                    self.selected_input_port = None; // Reset on error
                }
            } else {
                // Saved port not found - reset setting
                eprintln!("Saved input port '{}' is not available", input_port_name);
                self.settings_manager.update_midi_ports(None, self.selected_output_port.clone());
                self.selected_input_port = None;
            }
        }
        
        // Restore output port
        if let Some(output_port_name) = saved_output_port {
            // Check if the saved port exists in the current list of available ports
            if self.midi_output_ports.iter().any(|port| port == &output_port_name) {
                self.selected_output_port = Some(output_port_name.clone());
                
                // Try to connect to the port
                if let Err(e) = self.connect_output_port(&output_port_name) {
                    eprintln!("Output port restoration error '{}': {}", output_port_name, e);
                    self.selected_output_port = None; // Reset on error
                }
            } else {
                // Saved port not found - reset setting
                eprintln!("Saved output port '{}' is not available", output_port_name);
                self.settings_manager.update_midi_ports(self.selected_input_port.clone(), None);
                self.selected_output_port = None;
            }
        }
    }
    
    /// Restores the last preset from settings
    /// IMPORTANT: Only sets the preset name, but does NOT load its points,
    /// to preserve the actual curve state from settings
    fn restore_last_preset_from_settings(&mut self) {
        if let Some(preset_name) = self.settings_manager.get_last_preset().cloned() {
            // Only set selected_preset without loading the preset
            // This allows restoring the actual curve state from settings,
            // not from the preset (user could have made changes after preset loading)
            self.selected_preset = Some(preset_name);
        }
    }
    
    /// Saves current application settings
    fn save_settings(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Update curve state in settings
        self.settings_manager.update_from_dual_curve(&self.dual_curve.lock().unwrap());
        
        // Update MIDI ports
        self.settings_manager.update_midi_ports(
            self.selected_input_port.clone(),
            self.selected_output_port.clone()
        );
        
        // Update last preset
        self.settings_manager.update_last_preset(self.selected_preset.clone());
        
        // Save settings to file
        self.settings_manager.save()
    }
    
    /// Automatically saves settings if auto-save is enabled
    fn auto_save_settings(&mut self) {
        if self.settings_manager.is_auto_save_enabled() {
            if let Err(e) = self.save_settings() {
                eprintln!("Auto-save settings error: {}", e);
            }
        }
    }
    
    /// Called when closing application to save settings
    fn on_exit(&mut self) {
        if let Err(e) = self.save_settings() {
            eprintln!("Error saving settings: {}", e);
        }
    }
    
fn process_input_velocity(&self, input_velocity: u8) -> u8 {
        let mut dual_curve = self.dual_curve.lock().unwrap();
        // Use NoteOn curve by default for testing
        dual_curve.process_note_on_velocity(input_velocity)
    }
    
    // Curve testing
    fn test_curve(&mut self) {
        let input_velocity = 64; // Test value
        let _output_velocity = self.process_input_velocity(input_velocity);
        // Velocity test completed silently
    }
    
    // Update MIDI ports list
    fn refresh_midi_ports(&mut self) {
        // Use full MIDI manager
        let midi_manager = self.midi_manager.lock().unwrap();
        self.midi_input_ports = midi_manager.get_input_ports();
        self.midi_output_ports = midi_manager.get_output_ports();
        self.midi_stats = midi_manager.get_stats();
        
        // MIDI ports list updated silently
    }
    
    // Connect to input MIDI port
    fn connect_input_port(&mut self, port_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut midi_manager = self.midi_manager.lock().unwrap();
        midi_manager.connect_input_port(port_name)?;
        self.selected_input_port = Some(port_name.to_string());
        
        // Auto-save to settings
        if self.settings_manager.is_auto_save_enabled() {
            self.settings_manager.update_midi_ports(
                Some(port_name.to_string()),
                self.selected_output_port.clone()
            );
            let _ = self.settings_manager.save();
        }
        
        Ok(())
    }
    
    // Connect to output MIDI port
    fn connect_output_port(&mut self, port_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut midi_manager = self.midi_manager.lock().unwrap();
        midi_manager.connect_output_port(port_name)?;
        self.selected_output_port = Some(port_name.to_string());
        
        // Auto-save to settings
        if self.settings_manager.is_auto_save_enabled() {
            self.settings_manager.update_midi_ports(
                self.selected_input_port.clone(),
                Some(port_name.to_string())
            );
            let _ = self.settings_manager.save();
        }
        
        Ok(())
    }
    
// Disconnect all MIDI ports
    fn disconnect_all_ports(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut midi_manager = self.midi_manager.lock().unwrap();
        midi_manager.disconnect_all_ports()?;
        self.selected_input_port = None;
        self.selected_output_port = None;
        
        // Save settings
        if self.settings_manager.is_auto_save_enabled() {
            self.settings_manager.update_midi_ports(None, None);
            let _ = self.settings_manager.save();
        }
        
        Ok(())
    }
    
// Disconnect input MIDI port
    fn disconnect_input_port(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut midi_manager = self.midi_manager.lock().unwrap();
        midi_manager.disconnect_input_port()?;
        self.selected_input_port = None;
        
        // Save settings
        if self.settings_manager.is_auto_save_enabled() {
            self.settings_manager.update_midi_ports(None, self.selected_output_port.clone());
            let _ = self.settings_manager.save();
        }
        
        Ok(())
    }
    
    // Disconnect output MIDI port
    fn disconnect_output_port(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut midi_manager = self.midi_manager.lock().unwrap();
        midi_manager.disconnect_output_port()?;
        self.selected_output_port = None;
        
        // Save settings
        if self.settings_manager.is_auto_save_enabled() {
            self.settings_manager.update_midi_ports(self.selected_input_port.clone(), None);
            let _ = self.settings_manager.save();
        }
        
        Ok(())
    }
    
    // Check MIDI activity
    fn is_midi_active(&self) -> bool {
        let midi_manager = self.midi_manager.lock().unwrap();
        midi_manager.is_active()
    }
    
    // Update MIDI events list for GUI
    fn update_midi_events(&mut self) {
        // In real implementation events come through callbacks
        // For now leave empty for compatibility with simple manager
    }
    
    // Test MIDI processing
    fn test_midi_processing(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let _midi_manager = self.midi_manager.lock().unwrap();
        // Create test MIDI event
        let _test_event = MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 64,
            timestamp: std::time::Instant::now(),
        };
        
        // Generate test MIDI event silently
        
        Ok(())
    }
    
// Send test MIDI message to connected output port
    fn send_test_midi_message(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut dual_curve = self.dual_curve.lock().unwrap();
        
        // Create test MIDI NoteOn event
        let test_event = MidiEvent::NoteOn {
            channel: 0,
            note: 60, // Middle C
            velocity: 64, // Medium volume
            timestamp: std::time::Instant::now(),
        };
        
        // Apply NoteOn velocity curve to event
        let processed_event = {
            match test_event.clone() {
                MidiEvent::NoteOn { channel, note, velocity, timestamp } => {
                    let processed_velocity = dual_curve.process_note_on_velocity(velocity);
                    
                    MidiEvent::NoteOn {
                        channel,
                        note,
                        velocity: processed_velocity,
                        timestamp,
                    }
                }
                _ => test_event.clone(), // Other events not processed
            }
        };
        
        // Convert to MIDI data for sending
        let midi_data = processed_event.to_midi_data();
        
        // Send through full MIDI manager to output port
        let mut midi_manager = self.midi_manager.lock().unwrap();
        midi_manager.send_midi_data(&midi_data)?;
        
        // Also send corresponding NoteOff after some time
        std::thread::sleep(std::time::Duration::from_millis(200));
        
        let note_off_event = match test_event {
            MidiEvent::NoteOn { channel, note, velocity: _, timestamp: _ } => {
                // Apply NoteOff velocity curve to event
                let processed_note_off_velocity = dual_curve.process_note_off_velocity(64);
                
                MidiEvent::NoteOff {
                    channel,
                    note,
                    velocity: processed_note_off_velocity,
                    timestamp: std::time::Instant::now(),
                }
            }
            _ => unreachable!(), // Should be NoteOn
        };
        
        let note_off_data = note_off_event.to_midi_data();
        let _ = midi_manager.send_midi_data(&note_off_data);
        
        Ok(())
    }
    
// Load preset (applies to active tab)
    fn load_preset(&mut self, preset_name: &str) {
        // First get preset and clone its data
        let preset_data = {
            let preset_manager = self.preset_manager.lock().unwrap();
            preset_manager.get_preset(preset_name).map(|preset| preset.clone())
        };
        
        if let Some(preset) = preset_data {
            {
                let mut dual_curve = self.dual_curve.lock().unwrap();
                let points = preset.to_control_points();
                
                if self.active_tab_note_on {
                    // Apply to NoteOn curve
                    dual_curve.note_on_curve.control_points = points.clone();
                    dual_curve.note_on_curve.dirty = true;
                } else {
                    // Apply to NoteOff curve
                    dual_curve.note_off_curve.control_points = points;
                    dual_curve.note_off_curve.dirty = true;
                }
            } // release lock here
            
            self.selected_preset = Some(preset_name.to_string());
            
            // Auto-save changes
            self.auto_save_settings();
        }
    }
    
// Open save preset dialog
    fn open_save_preset_dialog(&mut self) {
        self.show_save_preset_dialog = true;
        self.save_preset_name.clear();
        self.save_preset_description.clear();
    }
    
    // Save preset from dialog
    fn save_preset_from_dialog(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let trimmed_name = self.save_preset_name.trim();
        
        if trimmed_name.is_empty() {
            return Err("Preset name cannot be empty".into());
        }
        
        // Check that preset with this name doesn't exist
        {
            let manager = self.preset_manager.lock().unwrap();
            if manager.get_preset(trimmed_name).is_some() {
                return Err(format!("Preset with name '{}' already exists", trimmed_name).into());
            }
        }
        
        let name = trimmed_name.to_string();
        let description = if self.save_preset_description.trim().is_empty() {
            format!("Preset {}, created {}",
                if self.active_tab_note_on { "NoteOn" } else { "NoteOff" },
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            )
        } else {
            self.save_preset_description.trim().to_string()
        };
        
        // Get curve points in advance to avoid borrowing issues
        let points = {
            let dual_curve = self.dual_curve.lock().unwrap();
            if self.active_tab_note_on {
                dual_curve.note_on_curve.control_points.clone()
            } else {
                dual_curve.note_off_curve.control_points.clone()
            }
        };
        
        let preset = CurvePreset::new(name.clone(), description, points);
        self.preset_manager.lock().unwrap().add_preset(preset)?;
        
        self.selected_preset = Some(name);
        self.show_save_preset_dialog = false;
        
        // Save settings after releasing lock
        let auto_save = self.settings_manager.is_auto_save_enabled();
        if auto_save {
            if let Err(e) = self.save_settings() {
                eprintln!("Auto-save settings error: {}", e);
            }
        }
        
        Ok(())
    }
    
    // Close save dialog without saving
    fn close_save_preset_dialog(&mut self) {
        self.show_save_preset_dialog = false;
        self.save_preset_name.clear();
        self.save_preset_description.clear();
    }
    
// Reset to linear curve (both curves)
fn reset_curve(&mut self) {
    {
        let mut dual_curve = self.dual_curve.lock().unwrap();
        dual_curve.reset_to_linear();
    } // release lock here
    
    self.selected_preset = None;
    self.selected_point = None;
    
    // Auto-save settings
    self.auto_save_settings();
}
    
// Add control point to active curve
fn add_control_point(&mut self, position: Pos2, rect: Rect) {
    let world_pos = self.screen_to_world(position, rect);
    {
        let mut dual_curve = self.dual_curve.lock().unwrap();
        
        if self.active_tab_note_on {
            dual_curve.add_note_on_point((world_pos.x, world_pos.y));
        } else {
            dual_curve.add_note_off_point((world_pos.x, world_pos.y));
        }
    } // release lock here
    
    // Reset selected_preset since curve was manually modified
    self.selected_preset = None;
    
    // Auto-save settings
    self.auto_save_settings();
}
    
// Remove selected point from active curve
fn remove_selected_point(&mut self) -> bool {
    if let Some(index) = self.selected_point {
        let removed = {
            let mut dual_curve = self.dual_curve.lock().unwrap();
            if self.active_tab_note_on {
                dual_curve.remove_note_on_point(index)
            } else {
                dual_curve.remove_note_off_point(index)
            }
        }; // release lock here
        
        if removed {
            self.selected_point = None;
            // Reset selected_preset since curve was manually modified
            self.selected_preset = None;
            // Auto-save settings
            self.auto_save_settings();
        }
        removed
    } else {
        false
    }
}
    
// Update point position in active curve
fn update_selected_point(&mut self, position: Pos2, rect: Rect) {
    if let Some(index) = self.selected_point {
        let mut world_pos = self.screen_to_world(position, rect);
        
        {
            let dual_curve = self.dual_curve.lock().unwrap();
            let points = if self.active_tab_note_on {
                &dual_curve.note_on_curve.control_points
            } else {
                &dual_curve.note_off_curve.control_points
            };
            
            // With fine movement (Shift) limit movement speed
            if self.shift_pressed {
                if let Some(current_point) = points.get(index) {
                    let current_x = current_point.position.0;
                    let current_y = current_point.position.1;
                    
                    let dx = world_pos.x - current_x;
                    let dy = world_pos.y - current_y;
                    
                    // In fine mode move very slowly
                    // Maximum step 0.02 per update (hundredths)
                    let max_fine_step = 0.02;
                    
                    world_pos = Pos2::new(
                        current_x + dx.signum() * dx.abs().min(max_fine_step),
                        current_y + dy.signum() * dy.abs().min(max_fine_step)
                    );
                }
            }
        }
        
        {
            let mut dual_curve = self.dual_curve.lock().unwrap();
            
            if self.active_tab_note_on {
                dual_curve.update_note_on_point(index, (world_pos.x, world_pos.y));
            } else {
                dual_curve.update_note_off_point(index, (world_pos.x, world_pos.y));
            }
        } // release lock here
        
        // Settings will be saved when dragging ends (mouse release)
    }
}
    
    // Convert screen coordinates to world coordinates
    fn screen_to_world(&self, screen_pos: Pos2, rect: Rect) -> Pos2 {
        let x = (screen_pos.x - rect.left()) / rect.width() * 127.0;
        let y = (rect.bottom() - screen_pos.y) / rect.height() * 127.0;
        Pos2::new(x, y)
    }
    
    // Convert world coordinates to screen coordinates
    fn world_to_screen(&self, world_pos: Pos2, rect: Rect) -> Pos2 {
        let x = rect.left() + world_pos.x / 127.0 * rect.width();
        let y = rect.bottom() - world_pos.y / 127.0 * rect.height();
        Pos2::new(x, y)
    }
    
fn find_point_at(&self, screen_pos: Pos2, rect: Rect) -> Option<usize> {
        const CLICK_RADIUS: f32 = 12.0;
        let dual_curve = self.dual_curve.lock().unwrap();
        
        let points = if self.active_tab_note_on {
            &dual_curve.note_on_curve.control_points
        } else {
            &dual_curve.note_off_curve.control_points
        };
        
        for (i, point) in points.iter().enumerate() {
            let point_screen = self.world_to_screen(Pos2::new(point.position.0, point.position.1), rect);
            if screen_pos.distance(point_screen) < CLICK_RADIUS {
                return Some(i);
            }
        }
        None
    }
}

impl eframe::App for MidiCurvesApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check if window close was requested
        if ctx.input(|i| i.viewport().close_requested()) {
            // Save settings before exit
            self.on_exit();
        }
        
        // Handle key presses
        self.shift_pressed = ctx.input(|i| i.modifiers.shift);
        
        // Draw main window
        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_main_ui(ui);
        });
        
        // Save preset dialog
        if self.show_save_preset_dialog {
            egui::Window::new("💾 Save Preset")
                .resizable(false)
                .collapsible(false)
                .default_pos(ctx.input(|i| i.pointer.hover_pos().unwrap_or(egui::pos2(400.0, 300.0))))
                .show(ctx, |ui| {
                    ui.label("Enter name for new preset:");
                    ui.add_space(5.0);
                    
                    // Name field
                    ui.vertical(|ui| {
                        ui.label("Preset name:");
                        ui.add(egui::TextEdit::singleline(&mut self.save_preset_name)
                            .desired_width(300.0)
                            .hint_text("My Preset"));
                    });
                    
                    ui.add_space(10.0);
                    
                    // Description field (optional)
                    ui.vertical(|ui| {
                        ui.label("Description (optional):");
    ui.add(egui::TextEdit::multiline(&mut self.save_preset_description)
                            .desired_width(300.0)
                            .hint_text("Preset description..."));
                    });
                    
                    ui.add_space(15.0);
                    
                    // Buttons
                    ui.horizontal(|ui| {
                        if ui.button("💾 Save").clicked() {
                            if let Err(e) = self.save_preset_from_dialog() {
                                eprintln!("Preset save error: {}", e);
                                // Could show error to user through separate field
                            }
                        }
                        
                        if ui.button("❌ Cancel").clicked() {
                            self.close_save_preset_dialog();
                        }
                    });
                });
        }
        
        // Normal repaint when needed (remove forced)
        // ctx.request_repaint();
    }
}

impl MidiCurvesApp {
    fn draw_main_ui(&mut self, ui: &mut egui::Ui) {
// Application title
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("🎵 MIDI Curves Plugin - Dual Curves").size(18.0));
        });
        
        ui.add_space(10.0);
        
// Tabs for selecting between NoteOn and NoteOff curves
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("📊 Curve Type:").size(14.0));
            ui.add_space(5.0);
            
            // NoteOn tab
            let note_on_button = ui.button(if self.active_tab_note_on {
                "🎵 NoteOn (Active)"
            } else {
                "🎵 NoteOn"
            });
            if note_on_button.clicked() {
                self.active_tab_note_on = true;
                self.selected_point = None; // Reset selected point when switching
                
                // Save active tab setting
                self.settings_manager.update_active_curve_tab(0);
                if self.settings_manager.is_auto_save_enabled() {
                    let _ = self.settings_manager.save();
                }
            }
            
            ui.add_space(5.0);
            
            // NoteOff tab
            let note_off_button = ui.button(if !self.active_tab_note_on {
                "🔇 NoteOff (Active)"
            } else {
                "🔇 NoteOff"
            });
            if note_off_button.clicked() {
                self.active_tab_note_on = false;
                self.selected_point = None; // Reset selected point when switching
                
                // Save active tab setting
                self.settings_manager.update_active_curve_tab(1);
                if self.settings_manager.is_auto_save_enabled() {
                    let _ = self.settings_manager.save();
                }
            }
        });
        
        ui.add_space(10.0);
        
        // Main layout with horizontal split
        ui.horizontal(|ui| {
            // Determine curve type for use in both panels
            let curve_type = if self.active_tab_note_on { "NoteOn" } else { "NoteOff" };
            
            // Left part - graph with title and controls
            ui.vertical(|ui| {
                ui.set_min_width(600.0);
                
                // Graph title
                ui.label(egui::RichText::new(format!("🎯 {} Curve Editor", curve_type)).size(16.0));
                
                // Graph area
                ui.add_space(5.0);
                let (response, painter) = ui.allocate_painter(
                    egui::vec2(600.0, 400.0),
                    Sense::click_and_drag(),
                );
                
                // Handle curve interaction
                self.handle_curve_interaction(&response);
                
                // Draw curve
                self.draw_curve(&painter, response.rect);
                
                // Remove point information from left panel - now it will be in right panel
                
                ui.add_space(10.0);
                
                // Selected point information
                if let Some(index) = self.selected_point {
                    let dual_curve = self.dual_curve.lock().unwrap();
                    let points = if self.active_tab_note_on {
                        &dual_curve.note_on_curve.control_points
                    } else {
                        &dual_curve.note_off_curve.control_points
                    };
                    
                    if let Some(point) = points.get(index) {
                        let format_str = if self.shift_pressed {
                            format!("🎯 Selected Point {}: ({:.2}, {:.2}) [FINE MODE]", index, point.position.0, point.position.1)
                        } else {
                            format!("🎯 Selected Point {}: ({:.1}, {:.1})", index, point.position.0, point.position.1)
                        };
                        ui.label(format_str);
                    }
} else {
    ui.label(format!("🎯 No point selected for {} curve - click on curve to select", curve_type));
}

ui.add_space(5.0);

// Curve work instructions
ui.label(egui::RichText::new("Double click - add point. Right click - delete point").size(12.0));

// Fine movement mode indication
if self.shift_pressed {
    ui.colored_label(
        egui::Color32::from_rgb(100, 200, 100),
        "🔧 FINE MODE: Shift held - precise positioning (0.01 increments)"
    );
} else {
    ui.colored_label(
        egui::Color32::from_gray(120),
        "💫 Hold Shift for fine control (0.01 increments)"
    );
}

ui.add_space(5.0);

// Control buttons
ui.horizontal(|ui| {
    if ui.button("🔄 Reset to Linear").clicked() {
        self.reset_curve();
    }
});
});

            // Right part - panels
            ui.vertical(|ui| {
                ui.set_min_width(400.0);
                
                // Add padding for alignment with graph title
                ui.add_space(31.0);
                
                // MIDI panel
                egui::Frame::group(ui.style())
                    .fill(egui::Color32::from_gray(30))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
                    .show(ui, |ui| {
                        self.draw_midi_panel(ui);
                    });
                
                ui.add_space(10.0);
                
                // Presets panel
                egui::Frame::group(ui.style())
                    .fill(egui::Color32::from_gray(30))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
                    .show(ui, |ui| {
                        self.draw_presets_panel(ui);
                    });
            });
        });
    }
    
    
    
    fn draw_test_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("🎯 Curve Test").size(12.0));
        ui.add_space(3.0);
        
        // Use interactive slider for test value
        let mut test_velocity = 64; // Initial value
        ui.horizontal(|ui| {
            ui.label("Velocity:");
            ui.add(egui::Slider::new(&mut test_velocity, 0..=127).show_value(false));
            ui.label(format!("{}", test_velocity));
        });
        
        let output_velocity = self.process_input_velocity(test_velocity);
        
        ui.add_space(5.0);
        
        // Visual indication
        ui.vertical(|ui| {
            let bar_width = 200.0;
            
            // Input value bar
            ui.horizontal(|ui| {
                ui.label("In:");
                let input_ratio = test_velocity as f32 / 127.0;
                ui.add_sized(
                    [bar_width, 12.0],
                    egui::widgets::ProgressBar::new(input_ratio)
                        .fill(egui::Color32::from_rgb(100, 100, 200))
                );
            });
            
            // Output value bar
            ui.horizontal(|ui| {
                ui.label("Out:");
                let output_ratio = output_velocity as f32 / 127.0;
                ui.add_sized(
                    [bar_width, 12.0],
                    egui::widgets::ProgressBar::new(output_ratio)
                        .fill(egui::Color32::from_rgb(100, 200, 100))
                );
            });
        });
        
        ui.add_space(3.0);
        
        if ui.button("🧪 Test").clicked() {
            self.test_curve();
        }
    }
    
fn draw_presets_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("📁 Presets").size(14.0));
        ui.add_space(5.0);
        
        // Get presets list
        let preset_names = self.preset_manager.lock().unwrap().get_preset_names();
        
        // Show currently selected preset
        if let Some(selected) = &self.selected_preset {
            ui.label(format!("Current: {}", selected));
        } else {
            ui.label("Current: (custom curve)");
        }
        
        ui.add_space(5.0);
        
        // Preset control buttons
        ui.horizontal(|ui| {
            // Save button now opens dialog
            if ui.button("💾 Save").clicked() {
                self.open_save_preset_dialog();
            }
            
// Delete button remains
            if ui.button("🗑️ Delete").clicked() {
                if let Some(selected_preset) = &self.selected_preset {
                    let should_auto_save = {
                        let mut manager = self.preset_manager.lock().unwrap();
                        let result = manager.remove_preset(selected_preset);
                        if let Err(e) = result {
                            eprintln!("Preset deletion error: {}", e);
                            false
                        } else {
                            true
                        }
                    };
                    
                    if should_auto_save {
                        self.selected_preset = None;
                        
                        // Save settings after releasing lock
                        let auto_save = self.settings_manager.is_auto_save_enabled();
                        if auto_save {
                            if let Err(e) = self.save_settings() {
                                eprintln!("Auto-save settings error: {}", e);
                            }
                        }
                    }
                }
            }
        });
        
        ui.add_space(5.0);
        
        // Available presets list with scrolling
        ui.label("Available Presets:");
        egui::ScrollArea::vertical()
            .max_height(120.0)
            .show(ui, |ui| {
                if preset_names.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_gray(120),
                        "No saved presets"
                    );
                } else {
                    for preset_name in preset_names {
                        let is_selected = self.selected_preset.as_ref() == Some(&preset_name);
                        let label_text = if is_selected {
                            format!("🎯 {}", preset_name)
                        } else {
                            preset_name.clone()
                        };
                        
                        if ui.selectable_label(is_selected, label_text).clicked() {
                            self.selected_preset = Some(preset_name.clone());
                            // Load preset immediately when clicking on name
                            self.load_preset(&preset_name);
                        }
                    }
                }
            });
    }
    
    fn draw_midi_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("🎹 MIDI Panel").size(12.0));
        ui.add_space(3.0);
        
        // Hi-Res MIDI toggle
        ui.group(|ui| {
            let mut hi_res_enabled = self.dual_curve.lock().unwrap().is_hi_res_enabled();
            
            if ui.checkbox(&mut hi_res_enabled, "Enable Hi-Res MIDI (14-bit)").clicked() {
                // Update state in dual_curve
                self.dual_curve.lock().unwrap().set_hi_res_enabled(hi_res_enabled);
                
                // Update in MIDI manager too (synchronization)
                // (dual_curve is already Arc, so changes will propagate automatically)
                
                // Save to settings
                self.settings_manager.set_hi_res_enabled(hi_res_enabled);
                self.auto_save_settings();
            }
            
            if hi_res_enabled {
                ui.colored_label(egui::Color32::from_rgb(100, 200, 100), "✓ Hi-Res: 14-bit (0-16383)");
            } else {
                ui.colored_label(egui::Color32::from_rgb(200, 200, 100), "Standard: 7-bit (0-127)");
            }
        });
        
        ui.add_space(10.0);
        
        // MIDI control buttons
        ui.horizontal(|ui| {
            if ui.button("🔄 Refresh").clicked() {
                self.refresh_midi_ports();
            }
            
            if ui.button("🎵 Test").clicked() {
                if let Err(e) = self.send_test_midi_message() {
                    eprintln!("MIDI test error: {}", e);
                }
            }
        });
        
        ui.add_space(10.0);
        
        // MIDI connection status
        let is_active = self.is_midi_active();
        let status_text = format!("Status: {}", if is_active { "Active" } else { "Inactive" });
        let status_color = if is_active {
            egui::Color32::from_rgb(100, 200, 100) // Green
        } else {
            egui::Color32::from_rgb(200, 100, 100) // Red
        };
        
        ui.colored_label(status_color, status_text);
        
        ui.add_space(5.0);
        
        // Input MIDI port selection dropdown
        ui.horizontal(|ui| {
            ui.label("📥 Input Port:");
            
            let selected_text = match &self.selected_input_port {
                Some(name) => name.clone(),
                None => "Not Selected".to_string(),
            };
            
            // Store selected port index for changes
            let mut input_port_changed = None;
            
            egui::ComboBox::from_id_source("input_port_selector")
                .selected_text(&selected_text)
                .show_ui(ui, |ui| {
                    // Disconnect option
                    if ui.selectable_label(self.selected_input_port.is_none(), "🔌 Disconnected").clicked() {
                        input_port_changed = Some(None);
                        ui.close_menu();
                    }
                    
                    // Available ports list
                    for (index, port_name) in self.midi_input_ports.iter().enumerate() {
                        let is_selected = self.selected_input_port.as_ref() == Some(port_name);
                        let display_text = if is_selected { "🔗 " } else { "📥 " };
                        
                        if ui.selectable_label(is_selected, format!("{}{}", display_text, port_name)).clicked() {
                            input_port_changed = Some(Some(index));
                            ui.close_menu();
                        }
                    }
                });
                
            // Handle selection changes after UI display
            if let Some(maybe_index) = input_port_changed {
                if let Some(index) = maybe_index {
                    // Clone port name before method calls
                    if let Some(port_name) = self.midi_input_ports.get(index) {
                        let port_name_cloned = port_name.clone();
                        match self.connect_input_port(&port_name_cloned) {
                            Ok(_) => {
                                self.selected_input_port = Some(port_name_cloned);
                            }
                            Err(e) => {
                                eprintln!("Input port connection error: {}", e);
                            }
                        }
                    }
                } else {
                    // Disconnect port
                    let _ = self.disconnect_input_port();
                }
            }
        });
        
        ui.add_space(3.0);
        
        // Output MIDI port selection dropdown
        ui.horizontal(|ui| {
            ui.label("📤 Output Port:");
            
            let selected_text = match &self.selected_output_port {
                Some(name) => name.clone(),
                None => "Not Selected".to_string(),
            };
            
            // Store selected port index for changes
            let mut output_port_changed = None;
            
            egui::ComboBox::from_id_source("output_port_selector")
                .selected_text(&selected_text)
                .show_ui(ui, |ui| {
                    // Disconnect option
                    if ui.selectable_label(self.selected_output_port.is_none(), "🔌 Disconnected").clicked() {
                        output_port_changed = Some(None);
                        ui.close_menu();
                    }
                    
                    // Available ports list
                    for (index, port_name) in self.midi_output_ports.iter().enumerate() {
                        let is_selected = self.selected_output_port.as_ref() == Some(port_name);
                        let display_text = if is_selected { "🔗 " } else { "📤 " };
                        
                        if ui.selectable_label(is_selected, format!("{}{}", display_text, port_name)).clicked() {
                            output_port_changed = Some(Some(index));
                            ui.close_menu();
                        }
                    }
                });
                
            // Handle selection changes after UI display
            if let Some(maybe_index) = output_port_changed {
                if let Some(index) = maybe_index {
                    // Clone port name before method calls
                    if let Some(port_name) = self.midi_output_ports.get(index) {
                        let port_name_cloned = port_name.clone();
                        match self.connect_output_port(&port_name_cloned) {
                            Ok(_) => {
                                self.selected_output_port = Some(port_name_cloned);
                            }
                            Err(e) => {
                                eprintln!("Output port connection error: {}", e);
                            }
                        }
                    }
                } else {
                    // Disconnect port
                    let _ = self.disconnect_output_port();
                }
            }
        });
        
        ui.add_space(5.0);
        
        // Disconnect all ports button
        if is_active && ui.button("🔌 Disconnect All").clicked() {
            if let Err(e) = self.disconnect_all_ports() {
                eprintln!("Port disconnection error: {}", e);
            }
        }
        
        if self.midi_input_ports.is_empty() && self.midi_output_ports.is_empty() {
            ui.add_space(3.0);
            ui.colored_label(egui::Color32::YELLOW, "⚠️ No MIDI ports found!");
        }
    }
    
    fn handle_curve_interaction(&mut self, response: &Response) {
        // Click to select point
        if response.clicked() {
            if let Some(hover_pos) = response.hover_pos() {
                self.selected_point = self.find_point_at(hover_pos, response.rect);
            }
        }
        
        // Point dragging
        if response.dragged() {
            if self.selected_point.is_some() {
                if let Some(hover_pos) = response.hover_pos() {
                    self.update_selected_point(hover_pos, response.rect);
                }
            }
        }
        
        // Track drag start
        if !self.is_dragging && response.dragged() && self.selected_point.is_some() {
            self.is_dragging = true;
        }
        
        // Save settings when mouse button released (drag release)
        if response.drag_stopped() && self.is_dragging {
            // Mouse released - point was moved
            self.is_dragging = false;
            // Reset selected_preset since curve was manually modified
            self.selected_preset = None;
            // Save settings
            self.auto_save_settings();
        }
        
        // Double click to add point
        if response.double_clicked() {
            if let Some(hover_pos) = response.hover_pos() {
                self.add_control_point(hover_pos, response.rect);
            }
        }
        
        // Right click to delete point
        if response.secondary_clicked() {
            if let Some(hover_pos) = response.hover_pos() {
                if let Some(point_idx) = self.find_point_at(hover_pos, response.rect) {
                    self.selected_point = Some(point_idx);
                    self.remove_selected_point();
                }
            }
        }
    }
    
    fn draw_curve(&mut self, painter: &Painter, rect: Rect) {
        // Draw grid
        self.draw_grid(painter, rect);
        
        // Draw Bezier curve with control points
        self.draw_bezier_curve(painter, rect);
    }
    
    fn draw_grid(&self, painter: &Painter, rect: Rect) {
        let grid_color = Color32::from_gray(40);
        
        // Vertical lines
        for i in 0..=10 {
            let x = rect.left() + rect.width() * i as f32 / 10.0;
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0, grid_color),
            );
        }
        
        // Horizontal lines
        for i in 0..=10 {
            let y = rect.top() + rect.height() * i as f32 / 10.0;
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(1.0, grid_color),
            );
        }
        
        // Axes
        let axis_color = Color32::from_gray(100);
        painter.line_segment(
            [Pos2::new(rect.left(), rect.bottom()), Pos2::new(rect.right(), rect.bottom())],
            Stroke::new(2.0, axis_color),
        );
        painter.line_segment(
            [Pos2::new(rect.left(), rect.top()), Pos2::new(rect.left(), rect.bottom())],
            Stroke::new(2.0, axis_color),
        );
    }
    
fn draw_bezier_curve(&mut self, painter: &Painter, rect: Rect) {
        let mut dual_curve = self.dual_curve.lock().unwrap();
        
        let curve = if self.active_tab_note_on {
            &mut dual_curve.note_on_curve
        } else {
            &mut dual_curve.note_off_curve
        };
        
        if curve.control_points.len() < 2 {
            return;
        }
        
        // Build curve points through interpolation
        let mut curve_points = Vec::new();
        
        // Generate curve points from 0 to 127 (MIDI velocity range)
        for i in 0..=128 {
            let x_input = i as f32;
            let y_output = curve.evaluate(x_input);
            
            // Convert to screen coordinates
            let screen_x = rect.left() + (x_input / 127.0) * rect.width();
            let screen_y = rect.bottom() - (y_output / 127.0) * rect.height();
            
            curve_points.push(Pos2::new(screen_x, screen_y));
        }
        
        // Draw curve as smooth line
        if curve_points.len() >= 2 {
            let curve_color = if self.active_tab_note_on {
                egui::Color32::from_rgb(100, 200, 255) // Blue for NoteOn
            } else {
                egui::Color32::from_rgb(255, 150, 100) // Orange for NoteOff
            };
            
            painter.add(egui::Shape::line(
                curve_points.clone(),
                egui::Stroke::new(3.0, curve_color)
            ));
        }
        
        // Draw control points as control elements
        for (i, point) in curve.control_points.iter().enumerate() {
            let screen_pos = self.world_to_screen(Pos2::new(point.position.0, point.position.1), rect);
            
            // Point color depends on selection and curve type
            let (color, stroke_color) = if Some(i) == self.selected_point {
                if self.active_tab_note_on {
                    (egui::Color32::from_rgb(255, 100, 100), egui::Color32::RED) // Red for selected NoteOn
                } else {
                    (egui::Color32::from_rgb(255, 150, 50), egui::Color32::from_rgb(255, 100, 0)) // Orange for selected NoteOff
                }
            } else {
                if self.active_tab_note_on {
                    (egui::Color32::from_rgb(255, 150, 150), egui::Color32::from_rgb(200, 100, 100)) // Pink for NoteOn
                } else {
                    (egui::Color32::from_rgb(255, 180, 120), egui::Color32::from_rgb(200, 120, 50)) // Light orange for NoteOff
                }
            };
            
            // Draw point
            painter.circle_filled(screen_pos, 6.0, color);
            painter.circle_stroke(screen_pos, 6.0, egui::Stroke::new(1.0, stroke_color));
            
            // Point number removed for VST3 compatibility (FontId causes panic before Context::run())
        }
    }
    
// Method draw_control_points removed - point drawing integrated into draw_bezier_curve
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 600.0])
            .with_title("MIDI Curves Plugin - Beta")
            .with_resizable(true)
            .with_fullscreen(false)
            .with_decorations(true),
        ..Default::default()
    };
    
    let app = MidiCurvesApp::new().unwrap();
    
    eframe::run_native(
        "MIDI Curves Plugin",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(app))
        }),
    )
}