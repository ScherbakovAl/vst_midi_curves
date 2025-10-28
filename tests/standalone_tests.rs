use vst_midi_curves::presets::{CurvePreset, PresetManager};
use vst_midi_curves::curve::{BezierCurve, ControlPoint};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[cfg(test)]
mod standalone_tests {
    use super::*;

    #[test]
    fn test_preset_manager_creation() {
        let mut manager = PresetManager::new().unwrap();
        manager.create_builtin_presets().unwrap();
        assert_eq!(manager.get_preset_names().len() > 0, true);
    }

    #[test]
    fn test_preset_serialization_cycle() {
        // Create test control points
        let original_points = vec![
            ControlPoint::new((0.0, 0.0)),
            ControlPoint::new((64.0, 32.0)),
            ControlPoint::new((127.0, 127.0)),
        ];
        
        // Create preset
        let preset = CurvePreset::new(
            "Test Serialization".to_string(),
            "Test preset for serialization".to_string(),
            original_points.clone(),
        );
        
        // Serialize and deserialize
        let temp_dir = TempDir::new().unwrap();
        let mut preset_path = temp_dir.path().to_path_buf();
        preset_path.push("test.json");
        
        preset.save_to_file(&preset_path).unwrap();
        let loaded_preset = CurvePreset::load_from_file(&preset_path).unwrap();
        
        // Check that data was saved
        assert_eq!(preset.name, loaded_preset.name);
        assert_eq!(preset.description, loaded_preset.description);
        assert_eq!(preset.control_points.len(), loaded_preset.control_points.len());
        
        // Check control points
        let restored_points = loaded_preset.to_control_points();
        for (original, restored) in original_points.iter().zip(restored_points.iter()) {
            assert_eq!(original.position, restored.position);
            assert_eq!(original.handle_in, restored.handle_in);
            assert_eq!(original.handle_out, restored.handle_out);
        }
    }

    #[test]
    fn test_bezier_curve_evaluation() {
        let mut curve = BezierCurve::new();
        
        // Test linear curve
        assert_eq!(curve.evaluate(0.0), 0.0);
        assert_eq!(curve.evaluate(64.0), 64.0);
        assert_eq!(curve.evaluate(127.0), 127.0);
        
        // Add point and test non-linear curve
        curve.add_control_point((64.0, 32.0));
        
        let value_64 = curve.evaluate(64.0);
        assert_ne!(value_64, 64.0); // Should be non-linear curve
        assert!(value_64 >= 0.0 && value_64 <= 127.0);
    }

    #[test]
    fn test_control_point_operations() {
        let mut curve = BezierCurve::new();
        
        // Test point addition
        let initial_count = curve.control_points.len();
        curve.add_control_point((64.0, 64.0));
        assert_eq!(curve.control_points.len(), initial_count + 1);
        
        // Test point update
        let update_result = curve.update_control_point(1, (32.0, 32.0));
        assert!(update_result);
        assert_eq!(curve.control_points[1].position, (32.0, 32.0));
        
        // Test point removal
        let remove_result = curve.remove_control_point(1);
        assert!(remove_result);
        assert_eq!(curve.control_points.len(), initial_count);
        
        // Cannot remove first or last point
        assert!(!curve.remove_control_point(0));
        assert!(!curve.remove_control_point(curve.control_points.len() - 1));
    }

    #[test]
    fn test_curve_reset() {
        // Create non-linear curve
        let mut curve = BezierCurve::new();
        curve.add_control_point((32.0, 16.0));
        curve.add_control_point((96.0, 111.0));
        
        // Check that curve is non-linear
        assert_ne!(curve.evaluate(64.0), 64.0);
        
        // Reset to linear
        curve.reset_to_linear();
        
        // Check that curve became linear
        assert_eq!(curve.evaluate(0.0), 0.0);
        assert_eq!(curve.evaluate(64.0), 64.0);
        assert_eq!(curve.evaluate(127.0), 127.0);
        assert_eq!(curve.control_points.len(), 2);
    }

    #[test]
    fn test_builtin_presets() {
        let mut manager = PresetManager::new().unwrap();
        manager.create_builtin_presets().unwrap();
        
        let preset_names = manager.get_preset_names();
        
        // Check that built-in presets were created
        assert!(preset_names.contains(&"Linear".to_string()));
        assert!(preset_names.contains(&"Soft S-Curve".to_string()));
        assert!(preset_names.contains(&"Exponential".to_string()));
        assert!(preset_names.contains(&"Inverse Exponential".to_string()));
        assert!(preset_names.contains(&"Sigmoid".to_string()));
        assert!(preset_names.contains(&"Hard Step".to_string()));
        assert!(preset_names.contains(&"Gentle Curve".to_string()));
        assert!(preset_names.contains(&"Aggressive Curve".to_string()));
        
        // Check that presets can be loaded
        let linear_preset = manager.get_preset("Linear").unwrap();
        let points = linear_preset.to_control_points();
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].position, (0.0, 0.0));
        assert_eq!(points[1].position, (127.0, 127.0));
    }

    #[test]
    fn test_error_handling() {
        // Test loading non-existent file
        let result = CurvePreset::load_from_file(&PathBuf::from("nonexistent.json"));
        assert!(result.is_err());
        
        // Test loading invalid JSON
        let temp_dir = TempDir::new().unwrap();
        let mut invalid_file = temp_dir.path().to_path_buf();
        invalid_file.push("invalid.json");
        fs::write(&invalid_file, "invalid json content").unwrap();
        
        let result = CurvePreset::load_from_file(&invalid_file);
        assert!(result.is_err());
    }
}