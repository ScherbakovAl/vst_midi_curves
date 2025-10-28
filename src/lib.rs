#![doc = "MIDI Curves VST3 Plugin"]

//! MIDI Curves VST3 Plugin
//!
//! VST3 plugin for MIDI velocity processing with customizable Bezier curves
//! Includes settings save/load system

use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, EguiState, egui};

use crate::curve::DualCurve;
use crate::midi_simple::SimpleMidiManager;
use crate::presets::PresetManager;
use crate::settings::SettingsManager;

// Import all necessary modules
mod curve;
mod presets;
mod midi;
mod midi_simple;
mod settings;

// Plugin parameters
#[derive(Params)]
struct MidiCurvesParams {
    /// Number of control points in the curve
    #[id = "control_points_count"]
    control_points_count: IntParam,
}

/// Buffer for hi-res MIDI messages in VST (CC#88 + NoteOn/Off)
/// Identical to HiResBuffer from standalone version
#[derive(Debug, Clone)]
struct VstHiResBuffer {
    /// Channel for which we buffer
    channel: Option<u8>,
    /// Lower 7 bits (from CC#88)
    lower_bits: Option<u8>,
    /// Time when CC#88 was received for timeout checking
    timestamp: Option<std::time::Instant>,
}

impl VstHiResBuffer {
    fn new() -> Self {
        Self {
            channel: None,
            lower_bits: None,
            timestamp: None,
        }
    }
    
    /// Stores CC#88 message (as in standalone)
    fn store_cc88(&mut self, channel: u8, lower_bits: u8) {
        self.channel = Some(channel);
        self.lower_bits = Some(lower_bits);
        self.timestamp = Some(std::time::Instant::now());
    }
    
    /// Extracts buffered value if available, channel matches and timeout hasn't expired
    /// IMPORTANT: Identical to standalone version with 100ms timeout check
    fn extract(&mut self, channel: u8) -> Option<u8> {
        if let (Some(buffered_channel), Some(lower)) = (self.channel, self.lower_bits) {
            // Check that channel matches
            if buffered_channel == channel {
                // Check timeout (100ms) - as in standalone
                if let Some(ts) = self.timestamp {
                    if ts.elapsed().as_millis() < 100 {
                        // Clear buffer and return value
                        self.clear();
                        return Some(lower);
                    }
                }
            }
        }
        // If not suitable - clear buffer
        self.clear();
        None
    }
    
    /// Clears the buffer
    fn clear(&mut self) {
        self.channel = None;
        self.lower_bits = None;
        self.timestamp = None;
    }
}

// Main plugin structure
struct MidiCurvesPlugin {
    /// Bezier curve processor (two curves: NoteOn and NoteOff)
    dual_curve_processor: Arc<Mutex<DualCurve>>,
    
    /// Simple MIDI manager
    midi_manager: Arc<Mutex<SimpleMidiManager>>,
    
    /// Preset system
    preset_manager: Arc<Mutex<PresetManager>>,
    
    /// Application settings manager
    settings_manager: SettingsManager,
    
    /// GUI state
    gui_state: Arc<Mutex<GuiState>>,
    
    /// Buffer for hi-res MIDI messages
    hi_res_buffer: Arc<Mutex<VstHiResBuffer>>,
}

// Structure for GUI state
struct GuiController {
    dual_curve_processor: Arc<Mutex<DualCurve>>,
    midi_manager: Arc<Mutex<SimpleMidiManager>>,
    preset_manager: Arc<Mutex<PresetManager>>,
    gui_state: Arc<Mutex<GuiState>>,
    settings_manager: SettingsManager,
}

// GUI state for VST3 editor
struct GuiState {
    selected_point: Option<usize>,
    is_dragging: bool,
    last_mouse_pos: Option<egui::Pos2>,
    active_tab_note_on: bool, // true for NoteOn, false for NoteOff
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            selected_point: None,
            is_dragging: false,
            last_mouse_pos: None,
            active_tab_note_on: true, // Default NoteOn
        }
    }
}

impl Default for MidiCurvesPlugin {
    fn default() -> Self {
        // VST3: SAFE initialization - use only memory, NO file operations
        // This prevents DAW crashes due to sandbox restrictions
        let settings_manager = SettingsManager::new_vst3_safe();
        
        // Restore DualCurve from settings or create new one
        let dual_curve_processor = Arc::new(Mutex::new(settings_manager.restore_to_dual_curve()));
        
        // Create MIDI manager
        let midi_manager = Arc::new(Mutex::new(SimpleMidiManager::new(dual_curve_processor.clone())));
        
        // VST3: Create preset system WITHOUT file system operations
        // Use empty manager and add built-in presets only in memory
        let preset_manager = Arc::new(Mutex::new(PresetManager::new_empty()));
        
        // Add built-in presets to memory (without saving to disk)
        {
            let mut preset_mgr = preset_manager.lock().unwrap();
            // Create built-in presets without saving to disk
            if let Err(e) = preset_mgr.create_builtin_presets_in_memory() {
                eprintln!("VST3: Error creating built-in presets: {}. Continuing without them.", e);
            }
        }
        
        // Restore hi_res setting from saved settings
        {
            let mut dual_curve = dual_curve_processor.lock().unwrap();
            dual_curve.set_hi_res_enabled(settings_manager.is_hi_res_enabled());
        }
        
        Self {
            dual_curve_processor,
            midi_manager,
            preset_manager,
            settings_manager: settings_manager.clone(),
            gui_state: Arc::new(Mutex::new(GuiState::default())),
            hi_res_buffer: Arc::new(Mutex::new(VstHiResBuffer::new())),
        }
    }
}

impl Plugin for MidiCurvesPlugin {
    const NAME: &'static str = "MIDI Curves";
    const VENDOR: &'static str = "Rust VST Developer";
    const URL: &'static str = "https://github.com/vst-midi-curves";
    const EMAIL: &'static str = "developer@vst-plugins.org";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // MIDI-only plugin with minimal stereo layout for Reaper compatibility
    // Reaper requires at least stereo input/output, even for MIDI-only plugins
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];

    // Configure MIDI configuration
    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::Basic;
    
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type BackgroundTask = ();
    type SysExMessage = ();

    fn params(&self) -> Arc<dyn Params> {
        // Create simple parameter structure
        Arc::new(MidiCurvesParams {
            control_points_count: IntParam::new(
                "Control Points",
                2,
                IntRange::Linear { min: 2, max: 16 },
            ),
        }) as Arc<dyn Params>
    }

fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
    // Clone Arc for use in closure
    let dual_curve_processor = self.dual_curve_processor.clone();
    let gui_state = self.gui_state.clone();
    let preset_manager = self.preset_manager.clone();
    let settings_manager = self.settings_manager.clone();
    
    create_egui_editor(
        EguiState::from_size(1000, 650),
        (),
        |_, _| {},
        move |egui_ctx, _setter, _state| {
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                // Get active tab once at the beginning
                let active_tab_note_on = {
                    let state = gui_state.lock().unwrap();
                    state.active_tab_note_on
                };
                
                // Title
                ui.horizontal(|ui| {
                    ui.heading("🎵 MIDI Curves VST3");
                    ui.add_space(20.0);
                    
                    // NoteOn/NoteOff curve switcher
                    ui.label("Curve Type:");
                    if ui.selectable_label(active_tab_note_on, "🎵 NoteOn").clicked() {
                        let mut state = gui_state.lock().unwrap();
                        state.active_tab_note_on = true;
                        state.selected_point = None;
                    }
                    if ui.selectable_label(!active_tab_note_on, "🔇 NoteOff").clicked() {
                        let mut state = gui_state.lock().unwrap();
                        state.active_tab_note_on = false;
                        state.selected_point = None;
                    }
                });
                
                ui.add_space(10.0);
                
                // Main layout
                ui.horizontal(|ui| {
                    // Left part - graph
                    ui.vertical(|ui| {
                        ui.set_min_width(650.0);
                        
                        ui.label("🎯 Curve Editor");
                        ui.add_space(5.0);
                        
                        // Create canvas for drawing with fixed size
                        let (response, painter) = ui.allocate_painter(
                            egui::vec2(600.0, 400.0),
                            egui::Sense::click_and_drag(),
                        );
                
                let rect = response.rect;
                
                // Draw background
                painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(40, 40, 50));
                
                // Calculate graph area
                let margin = 20.0;
                let graph_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + margin, rect.top() + margin),
                    egui::vec2(
                        rect.width() - margin * 2.0,
                        rect.height() - margin * 2.0
                    )
                );
                
                // Graph background
                painter.rect_filled(graph_rect, 0.0, egui::Color32::from_rgb(25, 25, 35));
                
                // MOUSE INTERACTION PROCESSING
                // Find point under cursor (copy points to quickly release lock)
                let hover_point: Option<usize> = if let Some(hover_pos) = response.hover_pos() {
                    const CLICK_RADIUS: f32 = 12.0;
                    
                    // Copy points and immediately release lock
                    let control_points_copy = {
                        let curve = dual_curve_processor.lock().unwrap();
                        if active_tab_note_on {
                            curve.note_on_curve.control_points.clone()
                        } else {
                            curve.note_off_curve.control_points.clone()
                        }
                    };
                    
                    let mut result = None;
                    for (i, point) in control_points_copy.iter().enumerate() {
                        let screen_x = graph_rect.left() + (point.position.0 / 127.0) * graph_rect.width();
                        let screen_y = graph_rect.bottom() - (point.position.1 / 127.0) * graph_rect.height();
                        let screen_pos = egui::pos2(screen_x, screen_y);
                        
                        if hover_pos.distance(screen_pos) < CLICK_RADIUS {
                            result = Some(i);
                            break;
                        }
                    }
                    result
                } else {
                    None
                };
                
                // Cursor handling (without locks to avoid deadlock)
                if response.hovered() {
                    let has_selected_point = {
                        let state = gui_state.lock().unwrap();
                        state.selected_point.is_some()
                    };
                    
                    if response.dragged() && has_selected_point {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    } else if hover_point.is_some() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    } else {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                    }
                }
                
                // Click to select point
                if response.clicked() {
                    let mut state = gui_state.lock().unwrap();
                    state.selected_point = hover_point;
                }
                
                // Right button for deletion (avoid simultaneous locking)
                if response.secondary_clicked() {
                    if let Some(point_index) = hover_point {
                        // First check if we can delete
                        let can_remove = {
                            let curve = dual_curve_processor.lock().unwrap();
                            if active_tab_note_on {
                                curve.note_on_curve.control_points.len() > 2
                            } else {
                                curve.note_off_curve.control_points.len() > 2
                            }
                        };
                        
                        if can_remove {
                            // Remove point
                            {
                                let mut curve = dual_curve_processor.lock().unwrap();
                                if active_tab_note_on {
                                    curve.remove_note_on_point(point_index);
                                } else {
                                    curve.remove_note_off_point(point_index);
                                }
                            }
                            
                            // Update state
                            {
                                let mut state = gui_state.lock().unwrap();
                                if state.selected_point == Some(point_index) {
                                    state.selected_point = None;
                                }
                            }
                        }
                    }
                }
                
                // Dragging (fixed lock order to avoid deadlock)
                if response.dragged() {
                    if let Some(hover_pos) = response.hover_pos() {
                        // First get selected_point without holding lock
                        let selected_index = {
                            let state = gui_state.lock().unwrap();
                            state.selected_point
                        };
                        
                        if let Some(selected_index) = selected_index {
                            let world_x = ((hover_pos.x - graph_rect.left()) / graph_rect.width() * 127.0).clamp(0.0, 127.0);
                            let world_y = ((graph_rect.bottom() - hover_pos.y) / graph_rect.height() * 127.0).clamp(0.0, 127.0);
                            
                            // Lock curve only after releasing gui_state
                            let mut curve = dual_curve_processor.lock().unwrap();
                            if active_tab_note_on {
                                curve.update_note_on_point(selected_index, (world_x, world_y));
                            } else {
                                curve.update_note_off_point(selected_index, (world_x, world_y));
                            }
                        }
                    }
                }
                
                // Double click to add
                if response.double_clicked() {
                    if let Some(hover_pos) = response.hover_pos() {
                        let world_x = ((hover_pos.x - graph_rect.left()) / graph_rect.width() * 127.0).clamp(0.0, 127.0);
                        let world_y = ((graph_rect.bottom() - hover_pos.y) / graph_rect.height() * 127.0).clamp(0.0, 127.0);
                        
                        let mut curve = dual_curve_processor.lock().unwrap();
                        if active_tab_note_on {
                            curve.add_note_on_point((world_x, world_y));
                        } else {
                            curve.add_note_off_point((world_x, world_y));
                        }
                    }
                }
                
                // Draw grid
                let grid_color = egui::Color32::from_gray(40);
                for i in 0..=10 {
                    let x = graph_rect.left() + graph_rect.width() * i as f32 / 10.0;
                    painter.line_segment(
                        [egui::pos2(x, graph_rect.top()), egui::pos2(x, graph_rect.bottom())],
                        egui::Stroke::new(1.0, grid_color),
                    );
                }
                for i in 0..=10 {
                    let y = graph_rect.top() + graph_rect.height() * i as f32 / 10.0;
                    painter.line_segment(
                        [egui::pos2(graph_rect.left(), y), egui::pos2(graph_rect.right(), y)],
                        egui::Stroke::new(1.0, grid_color),
                    );
                }
                
                // Axes
                let axis_color = egui::Color32::from_gray(100);
                painter.line_segment(
                    [egui::pos2(graph_rect.left(), graph_rect.bottom()), egui::pos2(graph_rect.right(), graph_rect.bottom())],
                    egui::Stroke::new(2.0, axis_color),
                );
                painter.line_segment(
                    [egui::pos2(graph_rect.left(), graph_rect.top()), egui::pos2(graph_rect.left(), graph_rect.bottom())],
                    egui::Stroke::new(2.0, axis_color),
                );
                
                        // Curve drawing
                        // CRITICAL OPTIMIZATION: Minimize lock time to prevent deadlock with audio thread
                        let selected_point = {
                            let state = gui_state.lock().unwrap();
                            state.selected_point
                        };
                        
                        // Calculate ALL curve points in one short lock
                        let (curve_points, control_points_copy) = {
                            let mut curve = dual_curve_processor.lock().unwrap();
                            let active_curve = if active_tab_note_on {
                                &mut curve.note_on_curve
                            } else {
                                &mut curve.note_off_curve
                            };
                            
                            let mut points = Vec::new();
                            if active_curve.control_points.len() >= 2 {
                                // Calculate all 129 curve points in one pass
                                for i in 0..=128 {
                                    let x_input = i as f32;
                                    let y_output = active_curve.evaluate(x_input);
                                    
                                    let screen_x = graph_rect.left() + (x_input / 127.0) * graph_rect.width();
                                    let screen_y = graph_rect.bottom() - (y_output / 127.0) * graph_rect.height();
                                    
                                    points.push(egui::pos2(screen_x, screen_y));
                                }
                            }
                            
                            // Copy control points
                            let control_copy = active_curve.control_points.clone();
                            
                            (points, control_copy)
                        }; // Lock released - now safe to draw
                        
                        // Drawing WITHOUT locks
                        if curve_points.len() >= 2 {
                            let curve_color = if active_tab_note_on {
                                egui::Color32::from_rgb(100, 200, 255)
                            } else {
                                egui::Color32::from_rgb(255, 150, 100)
                            };
                            
                            painter.add(egui::Shape::line(
                                curve_points,
                                egui::Stroke::new(3.0, curve_color)
                            ));
                        }
                        
                        // Control points
                        for (i, point) in control_points_copy.iter().enumerate() {
                            let screen_x = graph_rect.left() + (point.position.0 / 127.0) * graph_rect.width();
                            let screen_y = graph_rect.bottom() - (point.position.1 / 127.0) * graph_rect.height();
                            let screen_pos = egui::pos2(screen_x, screen_y);
                            
                            let color = if Some(i) == selected_point {
                                egui::Color32::from_rgb(255, 100, 100)
                            } else {
                                egui::Color32::from_rgb(255, 150, 150)
                            };
                            
                            painter.circle_filled(screen_pos, 6.0, color);
                            painter.circle_stroke(screen_pos, 6.0, egui::Stroke::new(1.0, egui::Color32::BLACK));
                        }
                        
                        ui.add_space(5.0);
                        
                        // Selected point information
                        // DEADLOCK FIX: Get selected_point, then lock curve
                        let selected_point = {
                            let state = gui_state.lock().unwrap();
                            state.selected_point
                        };
                        
                        if let Some(index) = selected_point {
                            let curve = dual_curve_processor.lock().unwrap();
                            
                            let control_points = if active_tab_note_on {
                                &curve.note_on_curve.control_points
                            } else {
                                &curve.note_off_curve.control_points
                            };
                            
                            if let Some(point) = control_points.get(index) {
                                let curve_type = if active_tab_note_on { "NoteOn" } else { "NoteOff" };
                                ui.label(format!(
                                    "🎯 Selected Point {} ({}): ({:.1}, {:.1})",
                                    index,
                                    curve_type,
                                    point.position.0,
                                    point.position.1
                                ));
                            }
                        } else {
                            let curve_type = if active_tab_note_on { "NoteOn" } else { "NoteOff" };
                            ui.label(format!("🎯 No point selected ({}) - click to select", curve_type));
                        }
                        
                        ui.label("Double click - add point | Right click - delete point");
                    });
                    
                    ui.add_space(10.0);
                    
                    // Right part - control panels
                    ui.vertical(|ui| {
                        ui.set_min_width(300.0);
                        
                        // Hi-Res MIDI panel
                        ui.group(|ui| {
                            ui.label("⚙️ MIDI Settings");
                            
                            // Get current state
                            let mut hi_res_enabled = dual_curve_processor.lock().unwrap().is_hi_res_enabled();
                            
                            // Checkbox - use changed() to track any changes
                            let response = ui.checkbox(&mut hi_res_enabled, "Enable Hi-Res MIDI (14-bit)");
                            
                            // If checkbox changed (clicked or programmatically)
                            if response.changed() {
                                // Update state in dual_curve
                                dual_curve_processor.lock().unwrap().set_hi_res_enabled(hi_res_enabled);
                                
                                // Save to settings
                                let mut settings_mgr = settings_manager.clone();
                                settings_mgr.set_hi_res_enabled(hi_res_enabled);
                                let _ = settings_mgr.save();
                                
                                // Debug output for verification
                                eprintln!("VST3: Hi-Res mode switched to: {}", hi_res_enabled);
                            }
                            
                            // Show current state
                            let current_hi_res = dual_curve_processor.lock().unwrap().is_hi_res_enabled();
                            
                            if current_hi_res {
                                ui.colored_label(egui::Color32::from_rgb(100, 200, 100), "✓ Hi-Res: 14-bit (0-16383)");
                                ui.label("Format: CC#88 (LL) + NoteOn/Off (HH)");
                            } else {
                                ui.colored_label(egui::Color32::from_rgb(200, 200, 100), "Standard: 7-bit (0-127)");
                            }
                        });
                        
                        ui.add_space(10.0);
                        
                        // Presets panel
                        ui.group(|ui| {
                            ui.label("📁 Presets");
                            
                            let preset_names = preset_manager.lock().unwrap().get_preset_names();
                            
                            ui.label("Available Presets:");
                            egui::ScrollArea::vertical()
                                .max_height(150.0)
                                .show(ui, |ui| {
                                    for preset_name in preset_names {
                                        if ui.button(&preset_name).clicked() {
                                            // Load preset only for active curve
                                            if let Some(preset) = preset_manager.lock().unwrap().get_preset(&preset_name) {
                                                let mut curve = dual_curve_processor.lock().unwrap();
                                                let points = preset.to_control_points();
                                                
                                                if active_tab_note_on {
                                                    curve.note_on_curve.control_points = points;
                                                    curve.note_on_curve.dirty = true;
                                                } else {
                                                    curve.note_off_curve.control_points = points;
                                                    curve.note_off_curve.dirty = true;
                                                }
                                            }
                                        }
                                    }
                                });
                        });
                        
                        ui.add_space(10.0);
                        
                        // Control buttons
                        ui.group(|ui| {
                            ui.label("🎛️ Controls");
                            
                            if ui.button("🔄 Reset to Linear").clicked() {
                                let mut curve = dual_curve_processor.lock().unwrap();
                                
                                if active_tab_note_on {
                                    curve.note_on_curve.reset_to_linear();
                                } else {
                                    curve.note_off_curve.reset_to_linear();
                                }
                                
                                let mut state = gui_state.lock().unwrap();
                                state.selected_point = None;
                            }
                        });
                        
                        ui.add_space(10.0);
                        
                        // Information
                        ui.group(|ui| {
                            ui.label("ℹ️ Info");
                            ui.label(format!("Version: {}", env!("CARGO_PKG_VERSION")));
                            ui.label("Platform: VST3");
                            
                            let curve = dual_curve_processor.lock().unwrap();
                            
                            let point_count = if active_tab_note_on {
                                curve.note_on_curve.control_points.len()
                            } else {
                                curve.note_off_curve.control_points.len()
                            };
                            
                            let curve_type = if active_tab_note_on { "NoteOn" } else { "NoteOff" };
                            ui.label(format!("Control Points ({}): {}", curve_type, point_count));
                        });
                    });
                });
            });
        },
    )
}

fn process(
        &mut self,
        _buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // OPTIMIZATION: Minimize lock time in audio thread
        // Process MIDI events
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::MidiCC {
                    timing,
                    channel,
                    cc,
                    value,
                    ..
                } => {
                    // Check CC#88 for hi-res mode (as in standalone)
                    if cc == 88 {
                        // Read hi-res state from dual_curve
                        let hi_res_enabled = {
                            let curve = self.dual_curve_processor.lock().unwrap();
                            curve.is_hi_res_enabled()
                        }; // Immediately release lock
                        
                        if hi_res_enabled {
                            // Hi-res enabled - save CC#88 to buffer (lower bits)
                            let ll = (value * 127.0) as u8;
                            self.hi_res_buffer.lock().unwrap().store_cc88(channel, ll);
                            
                            // Debug
                            eprintln!("VST3: Received CC#88={} on channel {}, buffered", ll, channel);
                        } else {
                            eprintln!("VST3: Received CC#88, but Hi-Res is OFF - ignoring");
                        }
                        // IMPORTANT: In both cases (hi-res enabled or not)
                        // DO NOT send incoming CC#88 further (as in standalone)
                        continue;
                    }
                    // Pass other CC messages unchanged
                    context.send_event(event);
                }
                
                NoteEvent::NoteOn {
                    timing,
                    voice_id,
                    channel,
                    note,
                    velocity,
                    ..
                } => {
                    // Minimize lock time - only during velocity processing
                    let (hi_res_enabled, processed_velocity_or_14bit) = {
                        let mut curve = self.dual_curve_processor.lock().unwrap();
                        let hi_res = curve.is_hi_res_enabled();
                        
                        if hi_res {
                            let lower_bits = self.hi_res_buffer.lock().unwrap().extract(channel);
                            let ll = lower_bits.unwrap_or(0);
                            let hh = (velocity * 127.0) as u8;
                            let velocity_14bit = crate::curve::DualCurve::combine_14bit(ll, hh);
                            let processed = curve.process_note_on_velocity_14bit(velocity_14bit);
                            
                            // Debug output
                            eprintln!("VST3 NoteOn: Hi-Res ON, LL={}, HH={}, 14bit_in={}, 14bit_out={}",
                                ll, hh, velocity_14bit, processed);
                            
                            (true, processed)
                        } else {
                            let processed = curve.process_note_on_velocity((velocity * 127.0) as u8) as u16;
                            (false, processed)
                        }
                    }; // Lock released
                    
                    if hi_res_enabled {
                        let (new_ll, new_hh) = crate::curve::DualCurve::split_14bit(processed_velocity_or_14bit);
                        
                        // Debug: check what we're sending
                        eprintln!("VST3: Sending CC#88={}, NoteOn velocity={}", new_ll, new_hh);
                        
                        context.send_event(NoteEvent::MidiCC {
                            timing,
                            channel,
                            cc: 88,
                            value: new_ll as f32 / 127.0,
                        });
                        
                        context.send_event(NoteEvent::NoteOn {
                            timing,
                            voice_id,
                            channel,
                            note,
                            velocity: new_hh as f32 / 127.0,
                        });
                    } else {
                        context.send_event(NoteEvent::NoteOn {
                            timing,
                            voice_id,
                            channel,
                            note,
                            velocity: processed_velocity_or_14bit as u8 as f32 / 127.0,
                        });
                    }
                }
                
                NoteEvent::NoteOff {
                    timing,
                    voice_id,
                    channel,
                    note,
                    velocity,
                    ..
                } => {
                    // Minimize lock time - only during velocity processing
                    let (hi_res_enabled, processed_velocity_or_14bit) = {
                        let mut curve = self.dual_curve_processor.lock().unwrap();
                        let hi_res = curve.is_hi_res_enabled();
                        
                        if hi_res {
                            let lower_bits = self.hi_res_buffer.lock().unwrap().extract(channel);
                            let ll = lower_bits.unwrap_or(0);
                            let hh = (velocity * 127.0) as u8;
                            let velocity_14bit = crate::curve::DualCurve::combine_14bit(ll, hh);
                            let processed = curve.process_note_off_velocity_14bit(velocity_14bit);
                            (true, processed)
                        } else {
                            let processed = curve.process_note_off_velocity((velocity * 127.0) as u8) as u16;
                            (false, processed)
                        }
                    }; // Lock released
                    
                    if hi_res_enabled {
                        let (new_ll, new_hh) = crate::curve::DualCurve::split_14bit(processed_velocity_or_14bit);
                        
                        context.send_event(NoteEvent::MidiCC {
                            timing,
                            channel,
                            cc: 88,
                            value: new_ll as f32 / 127.0,
                        });
                        
                        context.send_event(NoteEvent::NoteOff {
                            timing,
                            voice_id,
                            channel,
                            note,
                            velocity: new_hh as f32 / 127.0,
                        });
                    } else {
                        context.send_event(NoteEvent::NoteOff {
                            timing,
                            voice_id,
                            channel,
                            note,
                            velocity: processed_velocity_or_14bit as u8 as f32 / 127.0,
                        });
                    }
                }
                
                _ => {
                    // Pass all other events unchanged
                    context.send_event(event);
                }
            }
        }

        ProcessStatus::Normal
    }
}

// Implement required traits for VST3 plugin
impl Vst3Plugin for MidiCurvesPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"MidiCurvesVST3!!";
    // Category for MIDI plugin - Fx|MIDI for MIDI effects
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Fx,
        Vst3SubCategory::Tools,
    ];
}

impl ClapPlugin for MidiCurvesPlugin {
    const CLAP_ID: &'static str = "com.yourcompany.midicurves";
    const CLAP_MANUAL_URL: Option<&'static str> = Some("https://yourcompany.com");
    const CLAP_DESCRIPTION: Option<&'static str> = Some("MIDI velocity processing plugin");
    const CLAP_SUPPORT_URL: Option<&'static str> = Some("https://yourcompany.com");
    
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Utility
    ];
}

impl GuiController {
    /// Minimalist GUI drawing WITHOUT TEXT (no fonts required)
    /// Only graphics: Bezier curve with control points
    fn draw_minimal_gui(&self, ui: &mut egui::Ui) {
        // Create canvas for curve to full available size
        let available_size = ui.available_size();
        let (response, painter) = ui.allocate_painter(
            available_size,
            egui::Sense::click_and_drag(),
        );
        
        let rect = response.rect;
        
        // Draw canvas background
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(40, 40, 50));
        
        // Calculate graph area (with margins)
        let margin = 20.0;
        let graph_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + margin, rect.top() + margin),
            egui::vec2(
                rect.width() - margin * 2.0,
                rect.height() - margin * 2.0
            )
        );
        
        // Draw graph background
        painter.rect_filled(graph_rect, 0.0, egui::Color32::from_rgb(25, 25, 35));
        
        // Handle curve interactions
        self.handle_curve_interaction(&response);
        
        // Draw graphics (WITHOUT text - no fonts required!)
        self.draw_grid(&painter, graph_rect);
        self.draw_bezier_curve(&painter, graph_rect);
        
        // Test bright point in center for visibility check
        painter.circle_filled(
            graph_rect.center(),
            15.0,
            egui::Color32::from_rgb(255, 100, 100)
        );
    }
    
    /// Curve editor
    fn draw_curve_editor(&self, ui: &mut egui::Ui) {
        ui.label("🎯 Bezier Curve Editor");
        ui.add_space(5.0);
        
        // Canvas for graph
        let (response, painter) = ui.allocate_painter(
            egui::vec2(580.0, 500.0),
            egui::Sense::click_and_drag(),
        );
        
        // Handle curve interactions
        self.handle_curve_interaction(&response);
        
        // Draw curve
        self.draw_bezier_curve(&painter, response.rect);
        
        ui.add_space(10.0);
        
        // Selected point information
        // DEADLOCK FIX: First get selected_point, then lock curve
        let selected_point = {
            let gui_state = self.gui_state.lock().unwrap();
            gui_state.selected_point
        };
        
        if let Some(index) = selected_point {
            let curve = self.dual_curve_processor.lock().unwrap();
            if let Some(point) = curve.note_on_curve.control_points.get(index) {
                ui.label(format!(
                    "🎯 Selected point {}: ({:.1}, {:.1})",
                    index,
                    point.position.0,
                    point.position.1
                ));
            }
        } else {
            ui.label("🎯 No point selected - click on curve to select");
        }
        
        ui.add_space(5.0);
        
        // Point control buttons
        ui.horizontal(|ui| {
            if ui.button("➕ Add point").clicked() {
                if let Some(hover_pos) = response.hover_pos() {
                    let world_pos = self.screen_to_world(hover_pos, response.rect);
                    let mut curve = self.dual_curve_processor.lock().unwrap();
                    curve.add_note_on_point((world_pos.x, world_pos.y));
                }
            }
            
            if ui.button("❌ Delete point").clicked() {
                let index = {
                    let gui_state = self.gui_state.lock().unwrap();
                    gui_state.selected_point
                };
                
                if let Some(index) = index {
                    let mut curve = self.dual_curve_processor.lock().unwrap();
                    curve.remove_note_on_point(index);
                    
                    let mut gui_state = self.gui_state.lock().unwrap();
                    gui_state.selected_point = None;
                }
            }
            
            if ui.button("🔄 Reset to linear").clicked() {
                let mut curve = self.dual_curve_processor.lock().unwrap();
                curve.reset_to_linear();
                
                let mut gui_state = self.gui_state.lock().unwrap();
                gui_state.selected_point = None;
            }
        });
    }
    
    /// Control panel
    fn draw_control_panel(&self, ui: &mut egui::Ui) {
        // Curve test
        ui.group(|ui| {
            ui.label("🎯 Curve test");
            
            let mut test_velocity = 64.0;
            ui.horizontal(|ui| {
                ui.label("Velocity:");
                ui.add(egui::Slider::new(&mut test_velocity, 0.0..=127.0).show_value(false));
                ui.label(format!("{}", test_velocity as i32));
            });
            
            let output_velocity = {
                let mut curve = self.dual_curve_processor.lock().unwrap();
                curve.note_on_curve.evaluate(test_velocity) as i32
            };
            
            ui.add_space(5.0);
            
            // Visual indication
            ui.vertical(|ui| {
                let bar_width = 200.0;
                
                ui.horizontal(|ui| {
                    ui.label("In:");
                    let input_ratio = test_velocity / 127.0;
                    ui.add_sized(
                        [bar_width, 12.0],
                        egui::widgets::ProgressBar::new(input_ratio)
                            .fill(egui::Color32::from_rgb(100, 100, 200))
                    );
                });
                
                ui.horizontal(|ui| {
                    ui.label("Out:");
                    let output_ratio = output_velocity as f32 / 127.0;
                    ui.add_sized(
                        [bar_width, 12.0],
                        egui::widgets::ProgressBar::new(output_ratio)
                            .fill(egui::Color32::from_rgb(100, 200, 100))
                    );
                });
                
                ui.label(format!("Output: {}", output_velocity));
            });
        });
        
        ui.add_space(10.0);
        
        // Hi-Res MIDI settings panel
        ui.group(|ui| {
            ui.label("⚙️ MIDI Settings");
            
            let mut hi_res_enabled = self.dual_curve_processor.lock().unwrap().is_hi_res_enabled();
            
            if ui.checkbox(&mut hi_res_enabled, "Enable Hi-Res MIDI (14-bit velocity)").clicked() {
                // Update state in dual_curve
                self.dual_curve_processor.lock().unwrap().set_hi_res_enabled(hi_res_enabled);
                
                // Update in settings (in memory, without saving to disk in VST3)
                self.settings_manager.clone().set_hi_res_enabled(hi_res_enabled);
                // In VST3 auto-save to disk is disabled
                let _ = self.settings_manager.clone().save(); // No-op in VST3 mode
            }
            
            ui.add_space(5.0);
            
            if hi_res_enabled {
                ui.colored_label(egui::Color32::from_rgb(100, 200, 100), "✓ Hi-Res mode: 14-bit velocity (0-16383)");
                ui.label("Format: CC#88 (LL) + NoteOn/Off (HH)");
            } else {
                ui.colored_label(egui::Color32::from_rgb(200, 200, 100), "Standard mode: 7-bit velocity (0-127)");
            }
        });
        
        ui.add_space(10.0);
        
        // Presets panel
        ui.group(|ui| {
            ui.label("📁 Presets");
            
            let preset_names = self.preset_manager.lock().unwrap().get_preset_names();
            
            ui.horizontal(|ui| {
                ui.label("Selected preset:");
                
                egui::ComboBox::from_id_source("preset_selector")
                    .selected_text("Linear")
                    .show_ui(ui, |ui| {
                        for preset_name in preset_names {
                            if ui.selectable_label(false, &preset_name).clicked() {
                                // Load preset
                                let mut curve = self.dual_curve_processor.lock().unwrap();
                                if let Some(preset) = self.preset_manager.lock().unwrap().get_preset(&preset_name) {
                                    curve.load_from_preset(&preset);
                                }
                            }
                        }
                    });
            });
        });
        
        ui.add_space(10.0);
        
        // Plugin information
        ui.group(|ui| {
            ui.label("ℹ️ Information");
            ui.label(format!("Version: {}", env!("CARGO_PKG_VERSION")));
            ui.label("Platform: VST3 Standalone");
            
            let curve = self.dual_curve_processor.lock().unwrap();
            ui.label(format!("Control points: {}", curve.note_on_curve.control_points.len()));
        });
    }
    
    /// Handle curve interactions
    fn handle_curve_interaction(&self, response: &egui::Response) {
        // DEADLOCK FIX: Separate locks into individual operations
        
        // Click to select point
        if response.clicked() {
            if let Some(hover_pos) = response.hover_pos() {
                let selected_point = self.find_point_at(hover_pos, response.rect);
                let mut gui_state = self.gui_state.lock().unwrap();
                gui_state.selected_point = selected_point;
            }
        }
        
        // Dragging point
        if response.dragged() {
            // First get selected_point
            let selected_index = {
                let gui_state = self.gui_state.lock().unwrap();
                gui_state.selected_point
            };
            
            if let Some(selected_index) = selected_index {
                if let Some(hover_pos) = response.hover_pos() {
                    let world_pos = self.screen_to_world(hover_pos, response.rect);
                    let mut curve = self.dual_curve_processor.lock().unwrap();
                    curve.update_note_on_point(selected_index, (world_pos.x, world_pos.y));
                }
            }
        }
        
        // Track dragging start
        if response.dragged() {
            let mut gui_state = self.gui_state.lock().unwrap();
            if !gui_state.is_dragging && gui_state.selected_point.is_some() {
                gui_state.is_dragging = true;
            }
        }
        
        // Save settings on mouse button release (drag release)
        if response.drag_stopped() {
            let should_save = {
                let mut gui_state = self.gui_state.lock().unwrap();
                let was_dragging = gui_state.is_dragging;
                gui_state.is_dragging = false;
                was_dragging
            };
            
            if should_save {
                // Auto-save disabled in VST3 - settings stored only in memory
                let _ = self.auto_save_settings(); // Ignore result since it's no-op in VST3
            }
        }
        
        // Double click to add point
        if response.double_clicked() {
            if let Some(hover_pos) = response.hover_pos() {
                let world_pos = self.screen_to_world(hover_pos, response.rect);
                let mut curve = self.dual_curve_processor.lock().unwrap();
                curve.add_note_on_point((world_pos.x, world_pos.y));
            }
        }
    }
    
    /// Draw Bezier curve
    fn draw_bezier_curve(&self, painter: &egui::Painter, rect: egui::Rect) {
        // Draw grid
        self.draw_grid(painter, rect);
        
        // DEADLOCK FIX: First get selected_point, then work with curve
        let selected_point = {
            let gui_state = self.gui_state.lock().unwrap();
            gui_state.selected_point
        };
        
        let mut curve = self.dual_curve_processor.lock().unwrap();
        if curve.note_on_curve.control_points.len() < 2 {
            return;
        }
        
        // Generate curve points
        let mut curve_points = Vec::new();
        for i in 0..=128 {
            let x_input = i as f32;
            let y_output = curve.note_on_curve.evaluate(x_input);
            
            let screen_x = rect.left() + (x_input / 127.0) * rect.width();
            let screen_y = rect.bottom() - (y_output / 127.0) * rect.height();
            
            curve_points.push(egui::pos2(screen_x, screen_y));
        }
        
        // Draw curve
        if curve_points.len() >= 2 {
            painter.add(egui::Shape::line(
                curve_points,
                egui::Stroke::new(3.0, egui::Color32::from_rgb(100, 200, 255))
            ));
        }
        
        // Draw control points
        for (i, point) in curve.note_on_curve.control_points.iter().enumerate() {
            let screen_pos = self.world_to_screen(
                egui::pos2(point.position.0, point.position.1),
                rect
            );
            
            let color = if Some(i) == selected_point {
                egui::Color32::from_rgb(255, 100, 100)
            } else {
                egui::Color32::from_rgb(255, 150, 150)
            };
            
            painter.circle_filled(screen_pos, 6.0, color);
            painter.circle_stroke(screen_pos, 6.0, egui::Stroke::new(1.0, egui::Color32::BLACK));
            
            // Point number removed for VST3 compatibility (FontId causes panic before Context::run())
        }
    }
    
    /// Draw grid
    fn draw_grid(&self, painter: &egui::Painter, rect: egui::Rect) {
        let grid_color = egui::Color32::from_gray(40);
        
        // Vertical lines
        for i in 0..=10 {
            let x = rect.left() + rect.width() * i as f32 / 10.0;
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1.0, grid_color),
            );
        }
        
        // Horizontal lines
        for i in 0..=10 {
            let y = rect.top() + rect.height() * i as f32 / 10.0;
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(1.0, grid_color),
            );
        }
        
        // Axes
        let axis_color = egui::Color32::from_gray(100);
        painter.line_segment(
            [egui::pos2(rect.left(), rect.bottom()), egui::pos2(rect.right(), rect.bottom())],
            egui::Stroke::new(2.0, axis_color),
        );
        painter.line_segment(
            [egui::pos2(rect.left(), rect.top()), egui::pos2(rect.left(), rect.bottom())],
            egui::Stroke::new(2.0, axis_color),
        );
    }
    
    /// Convert screen coordinates to world coordinates
    fn screen_to_world(&self, screen_pos: egui::Pos2, rect: egui::Rect) -> egui::Pos2 {
        egui::pos2(
            (screen_pos.x - rect.left()) / rect.width() * 127.0,
            (rect.bottom() - screen_pos.y) / rect.height() * 127.0,
        )
    }
    
    /// Convert world coordinates to screen coordinates
    fn world_to_screen(&self, world_pos: egui::Pos2, rect: egui::Rect) -> egui::Pos2 {
        egui::pos2(
            rect.left() + world_pos.x / 127.0 * rect.width(),
            rect.bottom() - world_pos.y / 127.0 * rect.height(),
        )
    }
    
    /// Find point under cursor
    fn find_point_at(&self, screen_pos: egui::Pos2, rect: egui::Rect) -> Option<usize> {
        const CLICK_RADIUS: f32 = 12.0;
        let curve = self.dual_curve_processor.lock().unwrap();
        for (i, point) in curve.note_on_curve.control_points.iter().enumerate() {
            let point_screen = self.world_to_screen(
                egui::pos2(point.position.0, point.position.1),
                rect
            );
            if screen_pos.distance(point_screen) < CLICK_RADIUS {
                return Some(i);
            }
        }
        None
    }
}

// Plugin implementation
impl MidiCurvesPlugin {
    /// Saves current plugin settings
    /// In VST3 mode only updates settings in memory
    pub fn save_settings(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Update curve state in settings (in memory)
        self.settings_manager.update_from_dual_curve(&self.dual_curve_processor.lock().unwrap());
        
        // In VST3 this is no-op (doesn't write to disk), in standalone saves to file
        self.settings_manager.save()
    }
    
    /// Restores plugin settings from memory
    pub fn load_settings(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Restore DualCurve from settings (from memory in VST3)
        let restored_curve = self.settings_manager.restore_to_dual_curve();
        *self.dual_curve_processor.lock().unwrap() = restored_curve;
        
        Ok(())
    }
    
    /// Resets plugin to default settings (only in memory in VST3)
    pub fn reset_to_defaults(&mut self) {
        self.settings_manager.reset_to_default();
        
        // Apply reset settings
        let _ = self.load_settings();
    }
}

// GuiController implementation
impl GuiController {
    /// Automatically saves settings if auto-save is enabled
    /// In VST3 mode only updates settings in memory without writing to disk
    fn auto_save_settings(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Update curve state in settings (in memory)
        let dual_curve = self.dual_curve_processor.lock().unwrap();
        let mut settings_manager = self.settings_manager.clone();
        settings_manager.update_from_dual_curve(&dual_curve);
        
        // In VST3 mode save() is no-op (doesn't write to disk)
        // In standalone mode saves to file if auto-save is enabled
        if self.settings_manager.is_auto_save_enabled() {
            settings_manager.save()
        } else {
            Ok(())
        }
    }
}

// Plugin export
nih_plug::nih_export_vst3!(MidiCurvesPlugin);
