//! Module for working with curve presets
//! Provides saving, loading and management of user presets
//!
//! Copyright (c) 2025 Scherbakov Alexey <scherbakov.al@gmail.com>
//! Licensed under the MIT License

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

use crate::curve::ControlPoint;
use crate::curve::DualCurve;

/// Structure for preset serialization
#[derive(Serialize, Deserialize, Clone)]
pub struct CurvePreset {
    pub name: String,
    pub description: String,
    pub control_points: Vec<SerializableControlPoint>,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct SerializableControlPoint {
    pub x: f32,
    pub y: f32,
    pub handle_in_x: f32,
    pub handle_in_y: f32,
    pub handle_out_x: f32,
    pub handle_out_y: f32,
}

/// Structure for DualCurve preset serialization (two curves simultaneously)
#[derive(Serialize, Deserialize, Clone)]
pub struct DualCurvePreset {
    pub name: String,
    pub description: String,
    pub note_on_curve: Vec<SerializableControlPoint>,
    pub note_off_curve: Vec<SerializableControlPoint>,
}

impl DualCurvePreset {
    /// Creates new DualCurve preset from two sets of control points
    pub fn new(
        name: String,
        description: String,
        note_on_points: Vec<ControlPoint>,
        note_off_points: Vec<ControlPoint>,
    ) -> Self {
        let note_on_serializable = note_on_points.into_iter()
            .map(|point| SerializableControlPoint {
                x: point.position.0,
                y: point.position.1,
                handle_in_x: point.handle_in.0,
                handle_in_y: point.handle_in.1,
                handle_out_x: point.handle_out.0,
                handle_out_y: point.handle_out.1,
            })
            .collect();
            
        let note_off_serializable = note_off_points.into_iter()
            .map(|point| SerializableControlPoint {
                x: point.position.0,
                y: point.position.1,
                handle_in_x: point.handle_in.0,
                handle_in_y: point.handle_in.1,
                handle_out_x: point.handle_out.0,
                handle_out_y: point.handle_out.1,
            })
            .collect();
            
        Self {
            name,
            description,
            note_on_curve: note_on_serializable,
            note_off_curve: note_off_serializable,
        }
    }
    
    /// Converts DualCurve preset to DualCurve structure
    pub fn to_dual_curve(&self) -> DualCurve {
        let note_on_points = self.note_on_curve.iter()
            .map(|point| ControlPoint {
                position: (point.x, point.y),
                handle_in: (point.handle_in_x, point.handle_in_y),
                handle_out: (point.handle_out_x, point.handle_out_y),
            })
            .collect();
            
        let note_off_points = self.note_off_curve.iter()
            .map(|point| ControlPoint {
                position: (point.x, point.y),
                handle_in: (point.handle_in_x, point.handle_in_y),
                handle_out: (point.handle_out_x, point.handle_out_y),
            })
            .collect();
            
        DualCurve::from_points(note_on_points, note_off_points)
    }
    
    /// Saves DualCurve preset to file
    pub fn save_to_file(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        // Create directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
    
    /// Loads DualCurve preset from file
    pub fn load_from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        let preset = serde_json::from_str(&json)?;
        Ok(preset)
    }
    
    /// Returns path to DualCurve preset file
    pub fn get_preset_file_path(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let mut path = CurvePreset::get_presets_directory()?;
        path.push(format!("dual_{}.json", self.sanitize_filename()));
        Ok(path)
    }
    
    /// Cleans filename from invalid characters
    fn sanitize_filename(&self) -> String {
        self.name
            .chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                _ => c,
            })
            .collect()
    }
}
impl CurvePreset {
    /// Creates new preset from name and control points
    pub fn new(name: String, description: String, control_points: Vec<ControlPoint>) -> Self {
        let serializable_points = control_points.into_iter()
            .map(|point| SerializableControlPoint {
                x: point.position.0,
                y: point.position.1,
                handle_in_x: point.handle_in.0,
                handle_in_y: point.handle_in.1,
                handle_out_x: point.handle_out.0,
                handle_out_y: point.handle_out.1,
            })
            .collect();
            
        Self {
            name,
            description,
            control_points: serializable_points,
        }
    }
    
    /// Converts preset back to control points
    pub fn to_control_points(&self) -> Vec<ControlPoint> {
        self.control_points.iter()
            .map(|point| ControlPoint {
                position: (point.x, point.y),
                handle_in: (point.handle_in_x, point.handle_in_y),
                handle_out: (point.handle_out_x, point.handle_out_y),
            })
            .collect()
    }
    
    /// Saves preset to file
    pub fn save_to_file(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        // Create directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
    
    /// Loads preset from file
    pub fn load_from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        let preset = serde_json::from_str(&json)?;
        Ok(preset)
    }
    
    /// Returns path to presets directory
    pub fn get_presets_directory() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let mut presets_dir = dirs::config_dir()
            .ok_or("Failed to find configuration directory")?;
        
        presets_dir.push("vst_midi_curves");
        presets_dir.push("presets");
        
        Ok(presets_dir)
    }
    
    /// Returns path to preset file
    pub fn get_preset_file_path(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let mut path = Self::get_presets_directory()?;
        path.push(format!("{}.json", self.sanitize_filename()));
        Ok(path)
    }
    
    /// Cleans filename from invalid characters
    fn sanitize_filename(&self) -> String {
        self.name
            .chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                _ => c,
            })
            .collect()
    }
}

/// Preset manager with support for regular and DualCurve presets
pub struct PresetManager {
    presets: HashMap<String, CurvePreset>,
    dual_curve_presets: HashMap<String, DualCurvePreset>,
    presets_dir: PathBuf,
}

impl PresetManager {
    /// Creates new preset manager
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let presets_dir = CurvePreset::get_presets_directory()?;
        
        // Create directory if it doesn't exist
        fs::create_dir_all(&presets_dir)?;
        
        let mut manager = Self {
            presets: HashMap::new(),
            dual_curve_presets: HashMap::new(),
            presets_dir,
        };
        
        // Load existing presets
        manager.load_all_presets()?;
        
        Ok(manager)
    }
    
    /// Creates empty preset manager (for VST3 on initialization error)
    pub fn new_empty() -> Self {
        Self {
            presets: HashMap::new(),
            dual_curve_presets: HashMap::new(),
            presets_dir: PathBuf::new(),
        }
    }
    
    /// Adds preset
    pub fn add_preset(&mut self, preset: CurvePreset) -> Result<(), Box<dyn std::error::Error>> {
        let name = preset.name.clone();
        self.presets.insert(name.clone(), preset.clone());
        
        // Automatically save preset
        preset.save_to_file(&preset.get_preset_file_path()?)?;
        
        Ok(())
    }
    
    /// Gets preset by name
    pub fn get_preset(&self, name: &str) -> Option<&CurvePreset> {
        self.presets.get(name)
    }
    
    /// Removes preset
    pub fn remove_preset(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(preset) = self.presets.get(name) {
            let file_path = preset.get_preset_file_path()?;
            if file_path.exists() {
                fs::remove_file(file_path)?;
            }
        }
        
        self.presets.remove(name);
        Ok(())
    }
    
    /// Returns all preset names
    pub fn get_preset_names(&self) -> Vec<String> {
        self.presets.keys().cloned().collect()
    }
    
    /// Saves all presets to files
    pub fn save_all_presets(&self) -> Result<(), Box<dyn std::error::Error>> {
        for preset in self.presets.values() {
            preset.save_to_file(&preset.get_preset_file_path()?)?;
        }
        Ok(())
    }
    
    /// Loads all presets from files
    fn load_all_presets(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.presets_dir.exists() {
            return Ok(());
        }
        
        let entries = fs::read_dir(&self.presets_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(preset) = CurvePreset::load_from_file(&path) {
                    self.presets.insert(preset.name.clone(), preset);
                }
            }
        }
        
        Ok(())
    }
    
    /// Creates builtin presets in memory (without saving to disk)
    /// Safe for VST3 plugins running in sandbox
    pub fn create_builtin_presets_in_memory(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let builtin_presets = self.get_builtin_presets_data();
        
        for (name, description, control_points) in builtin_presets {
            if !self.presets.contains_key(&name) {
                let preset = CurvePreset::new(name.clone(), description, control_points);
                // Add only to memory, DO NOT save to disk
                self.presets.insert(name, preset);
            }
        }
        
        Ok(())
    }
    
    /// Creates builtin presets if they don't exist (with disk saving)
    /// Used only in standalone application
    pub fn create_builtin_presets(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let builtin_presets = vec![
            (
                "Linear".to_string(),
                "Linear curve (y = x)".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
            (
                "Soft S-Curve".to_string(),
                "Soft S-curve".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((42.0, 32.0)),
                    ControlPoint::new((85.0, 95.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
            (
                "Exponential".to_string(),
                "Exponential curve".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((25.0, 12.0)),
                    ControlPoint::new((63.0, 38.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
            (
                "Inverse Exponential".to_string(),
                "Inverse exponential curve".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((64.0, 89.0)),
                    ControlPoint::new((102.0, 115.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
            (
                "Sigmoid".to_string(),
                "Sigmoid curve".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((32.0, 16.0)),
                    ControlPoint::new((95.0, 111.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
            (
                "Hard Step".to_string(),
                "Hard step".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((64.0, 0.0)),
                    ControlPoint::new((64.0, 127.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
            (
                "Gentle Curve".to_string(),
                "Gentle curve".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((32.0, 20.0)),
                    ControlPoint::new((95.0, 107.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
            (
                "Aggressive Curve".to_string(),
                "Aggressive curve".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((48.0, 8.0)),
                    ControlPoint::new((80.0, 80.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
        ];
        
        let builtin_presets = self.get_builtin_presets_data();
        
        for (name, description, control_points) in builtin_presets {
            if !self.presets.contains_key(&name) {
                let preset = CurvePreset::new(name, description, control_points);
                self.add_preset(preset)?;
            }
        }
        
        Ok(())
    }
    
    /// Returns builtin presets data
    fn get_builtin_presets_data(&self) -> Vec<(String, String, Vec<ControlPoint>)> {
        vec![
            (
                "Linear".to_string(),
                "Linear curve (y = x)".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
            (
                "Soft S-Curve".to_string(),
                "Soft S-curve".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((42.0, 32.0)),
                    ControlPoint::new((85.0, 95.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
            (
                "Exponential".to_string(),
                "Exponential curve".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((25.0, 12.0)),
                    ControlPoint::new((63.0, 38.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
            (
                "Inverse Exponential".to_string(),
                "Inverse exponential curve".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((64.0, 89.0)),
                    ControlPoint::new((102.0, 115.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
            (
                "Sigmoid".to_string(),
                "Sigmoid curve".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((32.0, 16.0)),
                    ControlPoint::new((95.0, 111.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
            (
                "Hard Step".to_string(),
                "Hard step".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((64.0, 0.0)),
                    ControlPoint::new((64.0, 127.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
            (
                "Gentle Curve".to_string(),
                "Gentle curve".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((32.0, 20.0)),
                    ControlPoint::new((95.0, 107.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
            (
                "Aggressive Curve".to_string(),
                "Aggressive curve".to_string(),
                vec![
                    ControlPoint::new((0.0, 0.0)),
                    ControlPoint::new((48.0, 8.0)),
                    ControlPoint::new((80.0, 80.0)),
                    ControlPoint::new((127.0, 127.0)),
                ],
            ),
        ]
    }
}

impl Default for PresetManager {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            presets: HashMap::new(),
            dual_curve_presets: HashMap::new(),
            presets_dir: PathBuf::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_preset_serialization() {
        let points = vec![
            ControlPoint::new((0.0, 0.0)),
            ControlPoint::new((64.0, 32.0)),
            ControlPoint::new((127.0, 127.0)),
        ];
        
        let preset = CurvePreset::new(
            "Test Preset".to_string(),
            "Test description".to_string(),
            points.clone(),
        );
        
        let restored_points = preset.to_control_points();
        
        assert_eq!(restored_points.len(), points.len());
        for (original, restored) in points.iter().zip(restored_points.iter()) {
            assert_eq!(original.position, restored.position);
            assert_eq!(original.handle_in, restored.handle_in);
            assert_eq!(original.handle_out, restored.handle_out);
        }
    }
    
    #[test]
    fn test_preset_manager() {
        let mut manager = PresetManager::new().unwrap();
        
        let points = vec![
            ControlPoint::new((0.0, 0.0)),
            ControlPoint::new((127.0, 127.0)),
        ];
        
        let preset = CurvePreset::new(
            "Test".to_string(),
            "Test preset".to_string(),
            points,
        );
        
        manager.add_preset(preset).unwrap();
        assert!(manager.get_preset("Test").is_some());
        
        let names = manager.get_preset_names();
        assert!(names.contains(&"Test".to_string()));
    }
}