/// Module for saving and loading application settings
/// Supports cross-platform settings storage for Windows, macOS, Linux

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::curve::{DualCurve, ControlPoint};

/// Structure for saving control point state
#[derive(Serialize, Deserialize, Clone)]
struct SerializableControlPointData {
    x: f32,
    y: f32,
    handle_in_x: f32,
    handle_in_y: f32,
    handle_out_x: f32,
    handle_out_y: f32,
}

/// Main application settings structure
#[derive(Serialize, Deserialize, Clone)]
pub struct AppSettings {
    /// Last selected MIDI ports
    pub last_input_port: Option<String>,
    pub last_output_port: Option<String>,
    
    /// Curves state (NoteOn and NoteOff)
    pub note_on_points: Vec<SerializableControlPointData>,
    pub note_off_points: Vec<SerializableControlPointData>,
    
    /// Last loaded preset
    pub last_preset: Option<String>,
    
    /// GUI settings
    pub active_curve_tab: i32, // 0 = NoteOn, 1 = NoteOff
    
    /// Additional application settings
    pub auto_save_enabled: bool,
    pub show_advanced_controls: bool,
    
    /// High resolution MIDI mode (14-bit instead of 7-bit)
    pub hi_res_enabled: bool,
}

/// Application settings manager
#[derive(Clone)]
pub struct SettingsManager {
    settings: AppSettings,
    settings_path: PathBuf,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            last_input_port: None,
            last_output_port: None,
            note_on_points: vec![
                SerializableControlPointData {
                    x: 0.0, y: 0.0,
                    handle_in_x: 0.0, handle_in_y: 0.0,
                    handle_out_x: 0.0, handle_out_y: 0.0,
                },
                SerializableControlPointData {
                    x: 127.0, y: 127.0,
                    handle_in_x: 0.0, handle_in_y: 0.0,
                    handle_out_x: 0.0, handle_out_y: 0.0,
                },
            ],
            note_off_points: vec![
                SerializableControlPointData {
                    x: 0.0, y: 0.0,
                    handle_in_x: 0.0, handle_in_y: 0.0,
                    handle_out_x: 0.0, handle_out_y: 0.0,
                },
                SerializableControlPointData {
                    x: 127.0, y: 127.0,
                    handle_in_x: 0.0, handle_in_y: 0.0,
                    handle_out_x: 0.0, handle_out_y: 0.0,
                },
            ],
            last_preset: None,
            active_curve_tab: 0,
            auto_save_enabled: true,
            show_advanced_controls: false,
            hi_res_enabled: false,
        }
    }
}

impl SettingsManager {
    /// Creates new settings manager (for standalone version)
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let settings_path = Self::get_settings_path()?;
        
        // Create directory if it doesn't exist
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Load existing settings or create new ones
        let settings = if settings_path.exists() {
            Self::load_from_file(&settings_path)?
        } else {
            AppSettings::default()
        };
        
        Ok(Self {
            settings,
            settings_path,
        })
    }
    
    /// Creates settings manager for VST3 (memory only, NO file operations)
    pub fn new_vst3_safe() -> Self {
        Self {
            settings: AppSettings::default(),
            settings_path: PathBuf::new(), // Empty path - not used
        }
    }
    
    /// Saves current settings (for standalone version)
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Check if path is set (not VST3 mode)
        if self.settings_path.as_os_str().is_empty() {
            // VST3 mode - ignore save without error
            return Ok(());
        }
        
        let json = serde_json::to_string_pretty(&self.settings)?;
        fs::write(&self.settings_path, json)?;
        Ok(())
    }
    
    /// Checks if running in VST3 mode (memory only)
    pub fn is_vst3_mode(&self) -> bool {
        self.settings_path.as_os_str().is_empty()
    }
    
    /// Loads settings from file
    fn load_from_file(path: &Path) -> Result<AppSettings, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        let settings = serde_json::from_str(&json)?;
        Ok(settings)
    }
    
    /// Returns cross-platform path to settings file
    fn get_settings_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let mut config_dir = dirs::config_dir()
            .ok_or("Failed to find configuration directory")?;
        
        config_dir.push("vst_midi_curves");
        config_dir.push("settings.json");
        
        Ok(config_dir)
    }
    
    /// Updates MIDI ports state
    pub fn update_midi_ports(&mut self, input_port: Option<String>, output_port: Option<String>) {
        self.settings.last_input_port = input_port;
        self.settings.last_output_port = output_port;
    }
    
    /// Updates curves state from DualCurve
    pub fn update_from_dual_curve(&mut self, dual_curve: &DualCurve) {
        // Save NoteOn points
        self.settings.note_on_points = dual_curve.note_on_curve.control_points
            .iter()
            .map(|point| SerializableControlPointData {
                x: point.position.0,
                y: point.position.1,
                handle_in_x: point.handle_in.0,
                handle_in_y: point.handle_in.1,
                handle_out_x: point.handle_out.0,
                handle_out_y: point.handle_out.1,
            })
            .collect();
        
        // Save NoteOff points
        self.settings.note_off_points = dual_curve.note_off_curve.control_points
            .iter()
            .map(|point| SerializableControlPointData {
                x: point.position.0,
                y: point.position.1,
                handle_in_x: point.handle_in.0,
                handle_in_y: point.handle_in.1,
                handle_out_x: point.handle_out.0,
                handle_out_y: point.handle_out.1,
            })
            .collect();
        
        // Save Hi-Res mode state
        self.settings.hi_res_enabled = dual_curve.is_hi_res_enabled();
    }
    
    /// Restores DualCurve from saved settings
    pub fn restore_to_dual_curve(&self) -> DualCurve {
        // Restore NoteOn points
        let note_on_points = self.settings.note_on_points
            .iter()
            .map(|point| ControlPoint {
                position: (point.x, point.y),
                handle_in: (point.handle_in_x, point.handle_in_y),
                handle_out: (point.handle_out_x, point.handle_out_y),
            })
            .collect();
        
        // Restore NoteOff points
        let note_off_points = self.settings.note_off_points
            .iter()
            .map(|point| ControlPoint {
                position: (point.x, point.y),
                handle_in: (point.handle_in_x, point.handle_in_y),
                handle_out: (point.handle_out_x, point.handle_out_y),
            })
            .collect();
        
        // Create DualCurve with points
        let mut dual_curve = DualCurve::from_points(note_on_points, note_off_points);
        
        // Restore Hi-Res mode state from settings
        dual_curve.set_hi_res_enabled(self.settings.hi_res_enabled);
        
        dual_curve
    }
    
    /// Updates last loaded preset
    pub fn update_last_preset(&mut self, preset_name: Option<String>) {
        self.settings.last_preset = preset_name;
    }
    
    /// Updates active curve tab
    pub fn update_active_curve_tab(&mut self, tab_index: i32) {
        self.settings.active_curve_tab = tab_index;
    }
    
    /// Updates additional GUI settings
    pub fn update_gui_settings(&mut self, auto_save: bool, show_advanced: bool) {
        self.settings.auto_save_enabled = auto_save;
        self.settings.show_advanced_controls = show_advanced;
    }
    
    /// Updates high resolution MIDI mode
    pub fn set_hi_res_enabled(&mut self, enabled: bool) {
        self.settings.hi_res_enabled = enabled;
    }
    
    /// Gets high resolution MIDI mode state
    pub fn is_hi_res_enabled(&self) -> bool {
        self.settings.hi_res_enabled
    }
    
    /// Gets last selected input port
    pub fn get_last_input_port(&self) -> Option<&String> {
        self.settings.last_input_port.as_ref()
    }
    
    /// Gets last selected output port
    pub fn get_last_output_port(&self) -> Option<&String> {
        self.settings.last_output_port.as_ref()
    }
    
    /// Gets last loaded preset
    pub fn get_last_preset(&self) -> Option<&String> {
        self.settings.last_preset.as_ref()
    }
    
    /// Gets active curve tab
    pub fn get_active_curve_tab(&self) -> i32 {
        self.settings.active_curve_tab
    }
    
    /// Checks if auto-save is enabled
    pub fn is_auto_save_enabled(&self) -> bool {
        self.settings.auto_save_enabled
    }
    
    /// Checks whether to show advanced controls
    pub fn show_advanced_controls(&self) -> bool {
        self.settings.show_advanced_controls
    }
    
    /// Reset settings to default values
    pub fn reset_to_default(&mut self) {
        self.settings = AppSettings::default();
    }
    
    /// Export settings to JSON string
    pub fn export_settings(&self) -> Result<String, Box<dyn std::error::Error>> {
        Ok(serde_json::to_string_pretty(&self.settings)?)
    }
    
    /// Import settings from JSON string
    pub fn import_settings(&mut self, json: &str) -> Result<(), Box<dyn std::error::Error>> {
        let settings: AppSettings = serde_json::from_str(json)?;
        self.settings = settings;
        Ok(())
    }
}

impl Default for SettingsManager {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            settings: AppSettings::default(),
            settings_path: PathBuf::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;
    
    #[test]
    fn test_settings_creation() {
        let manager = SettingsManager::default();
        assert!(manager.get_last_input_port().is_none());
        assert!(manager.get_last_output_port().is_none());
        assert_eq!(manager.get_active_curve_tab(), 0);
        assert!(manager.is_auto_save_enabled());
    }
    
    #[test]
    fn test_midi_ports_update() {
        let mut manager = SettingsManager::default();
        manager.update_midi_ports(
            Some("Input Port 1".to_string()),
            Some("Output Port 1".to_string())
        );
        
        assert_eq!(manager.get_last_input_port(), Some(&"Input Port 1".to_string()));
        assert_eq!(manager.get_last_output_port(), Some(&"Output Port 1".to_string()));
    }
    
    #[test]
    fn test_preset_update() {
        let mut manager = SettingsManager::default();
        manager.update_last_preset(Some("Linear".to_string()));
        
        assert_eq!(manager.get_last_preset(), Some(&"Linear".to_string()));
    }
    
    #[test]
    fn test_dual_curve_save_restore() {
        use crate::curve::{DualCurve, ControlPoint};
        
        let mut manager = SettingsManager::default();
        
        // Create DualCurve with test points
        let mut dual_curve = DualCurve::new();
        dual_curve.add_note_on_point((50.0, 60.0));
        dual_curve.add_note_off_point((70.0, 80.0));
        
        // Save to settings
        manager.update_from_dual_curve(&dual_curve);
        
        // Restore and check
        let restored_curve = manager.restore_to_dual_curve();
        
        assert_eq!(restored_curve.note_on_curve.control_points.len(), 3); // 2 base + 1 added
        assert_eq!(restored_curve.note_off_curve.control_points.len(), 3); // 2 base + 1 added
    }
    
    #[test]
    fn test_settings_export_import() {
        let mut manager = SettingsManager::default();
        manager.update_last_preset(Some("Test Preset".to_string()));
        
        let exported = manager.export_settings().unwrap();
        let mut new_manager = SettingsManager::default();
        new_manager.import_settings(&exported).unwrap();
        
        assert_eq!(new_manager.get_last_preset(), Some(&"Test Preset".to_string()));
    }
}