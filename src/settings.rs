/// Модуль для сохранения и загрузки настроек приложения
/// Поддерживает кроссплатформенное хранение настроек для Windows, macOS, Linux

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::curve::{DualCurve, ControlPoint};

/// Структура для сохранения состояния контрольной точки
#[derive(Serialize, Deserialize, Clone)]
struct SerializableControlPointData {
    x: f32,
    y: f32,
    handle_in_x: f32,
    handle_in_y: f32,
    handle_out_x: f32,
    handle_out_y: f32,
}

/// Основная структура настроек приложения
#[derive(Serialize, Deserialize, Clone)]
pub struct AppSettings {
    /// Последние выбранные MIDI порты
    pub last_input_port: Option<String>,
    pub last_output_port: Option<String>,
    
    /// Состояние кривых (NoteOn и NoteOff)
    pub note_on_points: Vec<SerializableControlPointData>,
    pub note_off_points: Vec<SerializableControlPointData>,
    
    /// Последний загруженный пресет
    pub last_preset: Option<String>,
    
    /// GUI настройки
    pub active_curve_tab: i32, // 0 = NoteOn, 1 = NoteOff
    
    /// Дополнительные настройки приложения
    pub auto_save_enabled: bool,
    pub show_advanced_controls: bool,
    
    /// Режим MIDI высокого разрешения (14-бит вместо 7-бит)
    pub hi_res_enabled: bool,
}

/// Менеджер настроек приложения
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
    /// Создает новый менеджер настроек
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let settings_path = Self::get_settings_path()?;
        
        // Создаем директорию если её нет
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Загружаем существующие настройки или создаем новые
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
    
    /// Сохраняет текущие настройки
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string_pretty(&self.settings)?;
        fs::write(&self.settings_path, json)?;
        Ok(())
    }
    
    /// Загружает настройки из файла
    fn load_from_file(path: &Path) -> Result<AppSettings, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        let settings = serde_json::from_str(&json)?;
        Ok(settings)
    }
    
    /// Возвращает кроссплатформенный путь к файлу настроек
    fn get_settings_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let mut config_dir = dirs::config_dir()
            .ok_or("Не удалось найти директорию конфигурации")?;
        
        config_dir.push("vst_midi_curves");
        config_dir.push("settings.json");
        
        Ok(config_dir)
    }
    
    /// Обновляет состояние MIDI портов
    pub fn update_midi_ports(&mut self, input_port: Option<String>, output_port: Option<String>) {
        self.settings.last_input_port = input_port;
        self.settings.last_output_port = output_port;
    }
    
    /// Обновляет состояние кривых из DualCurve
    pub fn update_from_dual_curve(&mut self, dual_curve: &DualCurve) {
        // Сохраняем точки NoteOn
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
        
        // Сохраняем точки NoteOff
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
    }
    
    /// Восстанавливает DualCurve из сохраненных настроек
    pub fn restore_to_dual_curve(&self) -> DualCurve {
        // Восстанавливаем точки NoteOn
        let note_on_points = self.settings.note_on_points
            .iter()
            .map(|point| ControlPoint {
                position: (point.x, point.y),
                handle_in: (point.handle_in_x, point.handle_in_y),
                handle_out: (point.handle_out_x, point.handle_out_y),
            })
            .collect();
        
        // Восстанавливаем точки NoteOff
        let note_off_points = self.settings.note_off_points
            .iter()
            .map(|point| ControlPoint {
                position: (point.x, point.y),
                handle_in: (point.handle_in_x, point.handle_in_y),
                handle_out: (point.handle_out_x, point.handle_out_y),
            })
            .collect();
        
        DualCurve::from_points(note_on_points, note_off_points)
    }
    
    /// Обновляет последний загруженный пресет
    pub fn update_last_preset(&mut self, preset_name: Option<String>) {
        self.settings.last_preset = preset_name;
    }
    
    /// Обновляет активную вкладку кривой
    pub fn update_active_curve_tab(&mut self, tab_index: i32) {
        self.settings.active_curve_tab = tab_index;
    }
    
    /// Обновляет дополнительные настройки GUI
    pub fn update_gui_settings(&mut self, auto_save: bool, show_advanced: bool) {
        self.settings.auto_save_enabled = auto_save;
        self.settings.show_advanced_controls = show_advanced;
    }
    
    /// Обновляет режим MIDI высокого разрешения
    pub fn set_hi_res_enabled(&mut self, enabled: bool) {
        self.settings.hi_res_enabled = enabled;
    }
    
    /// Получает состояние режима MIDI высокого разрешения
    pub fn is_hi_res_enabled(&self) -> bool {
        self.settings.hi_res_enabled
    }
    
    /// Получает последний выбранный входной порт
    pub fn get_last_input_port(&self) -> Option<&String> {
        self.settings.last_input_port.as_ref()
    }
    
    /// Получает последний выбранный выходной порт
    pub fn get_last_output_port(&self) -> Option<&String> {
        self.settings.last_output_port.as_ref()
    }
    
    /// Получает последний загруженный пресет
    pub fn get_last_preset(&self) -> Option<&String> {
        self.settings.last_preset.as_ref()
    }
    
    /// Получает активную вкладку кривой
    pub fn get_active_curve_tab(&self) -> i32 {
        self.settings.active_curve_tab
    }
    
    /// Проверяет включено ли автосохранение
    pub fn is_auto_save_enabled(&self) -> bool {
        self.settings.auto_save_enabled
    }
    
    /// Проверяет показывать ли расширенные элементы управления
    pub fn show_advanced_controls(&self) -> bool {
        self.settings.show_advanced_controls
    }
    
    /// Сброс настроек к значениям по умолчанию
    pub fn reset_to_default(&mut self) {
        self.settings = AppSettings::default();
    }
    
    /// Экспорт настроек в JSON строку
    pub fn export_settings(&self) -> Result<String, Box<dyn std::error::Error>> {
        Ok(serde_json::to_string_pretty(&self.settings)?)
    }
    
    /// Импорт настроек из JSON строки
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
        
        // Создаем DualCurve с тестовыми точками
        let mut dual_curve = DualCurve::new();
        dual_curve.add_note_on_point((50.0, 60.0));
        dual_curve.add_note_off_point((70.0, 80.0));
        
        // Сохраняем в настройки
        manager.update_from_dual_curve(&dual_curve);
        
        // Восстанавливаем и проверяем
        let restored_curve = manager.restore_to_dual_curve();
        
        assert_eq!(restored_curve.note_on_curve.control_points.len(), 3); // 2 базовых + 1 добавленная
        assert_eq!(restored_curve.note_off_curve.control_points.len(), 3); // 2 базовых + 1 добавленная
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