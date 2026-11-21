//! Настройки standalone: порты, карты нот обеих кривых, формат выхода,
//! вкладка — `~/.config/vst_midi_curves/settings.json`.
//!
//! Чтение не паникует: битый файл — значения по умолчанию и сообщение.
//! Старые форматы переводятся: 1 (до 2026-11: `note_on_points`/
//! `note_off_points` в единицах 0…127, `hi_res_enabled`) — точной миграцией
//! формы, 2 (одна форма на кривую) — картой из одной ключевой ноты. Перед
//! первой перезаписью старого файла рядом остаётся его копия
//! `settings.v<версия>.json` (старая программа новый формат не читает).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::curve::legacy::{shape_from_legacy, LegacyPoint};
use crate::curve::{CurveSet, NoteMap};

/// Версия формата файла настроек (1 — старый формат без поля `version`,
/// 2 — одна форма на кривую, 3 — карты нот).
pub const SETTINGS_VERSION: u32 = 3;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub last_input_port: Option<String>,
    pub last_output_port: Option<String>,
    pub curves: CurveSet,
    /// 0 — Note On, 1 — Note Off.
    pub active_curve_tab: u8,
}

/// Поля старого формата (только чтение).
#[derive(Deserialize, Default)]
#[serde(default)]
struct LegacyFields {
    note_on_points: Option<Vec<LegacyPoint>>,
    note_off_points: Option<Vec<LegacyPoint>>,
    hi_res_enabled: Option<bool>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            last_input_port: None,
            last_output_port: None,
            curves: CurveSet::default(),
            active_curve_tab: 0,
        }
    }
}

/// Версия формата в тексте файла (нет поля — 1).
fn file_version(text: &str) -> u32 {
    #[derive(Deserialize)]
    struct V {
        version: Option<u32>,
    }
    serde_json::from_str::<V>(text).ok().and_then(|v| v.version).unwrap_or(1)
}

/// Путь по умолчанию; `None` — у системы нет папки настроек.
pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("vst_midi_curves").join("settings.json"))
}

impl Settings {
    /// Настройки из текста; старый формат переводится. Формы чинятся.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut s: Settings = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let old: LegacyFields = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if let Some(p) = old.note_on_points {
            s.curves.note_on = NoteMap::single(shape_from_legacy(&p));
        }
        if let Some(p) = old.note_off_points {
            s.curves.note_off = NoteMap::single(shape_from_legacy(&p));
        }
        if let Some(h) = old.hi_res_enabled {
            s.curves.hi_res = h;
        }
        s.version = SETTINGS_VERSION;
        s.active_curve_tab = s.active_curve_tab.min(1);
        s.curves.normalize();
        Ok(s)
    }

    /// Прочитать файл. Нет файла — умолчания без сообщения; не читается или
    /// битый — умолчания и текст ошибки для GUI.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Self::default(), None),
            Err(e) => return (Self::default(), Some(format!("{}: {e}", path.display()))),
        };
        match Self::parse(&text) {
            Ok(s) => {
                let v = file_version(&text);
                if v < SETTINGS_VERSION {
                    // Старый формат: копия до первой перезаписи.
                    let backup = path.with_file_name(format!("settings.v{v}.json"));
                    if !backup.exists() {
                        let _ = std::fs::copy(path, backup);
                    }
                }
                (s, None)
            }
            Err(e) => (Self::default(), Some(format!("{}: {e} — defaults used", path.display()))),
        }
    }

    /// Записать атомарно (временный файл + переименование).
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::legacy::legacy_eval;

    #[test]
    fn legacy_settings_migrate_exactly() {
        let text = include_str!("../tests/fixtures/legacy/settings.json");
        let s = Settings::parse(text).unwrap();
        assert_eq!(s.version, SETTINGS_VERSION);
        assert!(s.curves.hi_res);
        assert_eq!(s.last_output_port.as_deref(), Some("Midi Through:Midi Through Port-0 14:0"));
        let old: serde_json::Value = serde_json::from_str(text).unwrap();
        let on: Vec<LegacyPoint> = serde_json::from_value(old["note_on_points"].clone()).unwrap();
        for j in 0..=127 * 4 {
            let x = j as f32 / 4.0;
            let d = (s.curves.note_on.shape_at(60).eval(x / 127.0) * 127.0 - legacy_eval(&on, x)).abs();
            assert!(d < 0.01, "x={x}: {d}");
        }
        // В новом формате старых полей нет, и он читается обратно как есть.
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("note_on_points") && !json.contains("hi_res_enabled"));
        assert_eq!(Settings::parse(&json).unwrap(), s);
    }

    /// Формат 2 (настройки пользователя на 2026-11-21): форма каждой кривой
    /// становится картой из одной ключевой — бит-в-бит та же форма.
    #[test]
    fn settings_v2_migrate_to_single_key_maps() {
        let text = include_str!("../tests/fixtures/v2/settings.json");
        let s = Settings::parse(text).unwrap();
        assert_eq!(s.version, SETTINGS_VERSION);
        assert!(s.curves.hi_res);
        let old: serde_json::Value = serde_json::from_str(text).unwrap();
        for (map, key) in [(&s.curves.note_on, "note_on"), (&s.curves.note_off, "note_off")] {
            let shape: crate::curve::CurveShape = serde_json::from_value(old["curves"][key].clone()).unwrap();
            assert_eq!(map, &NoteMap::single(shape));
        }
        assert_eq!(s.last_output_port, old["last_output_port"].as_str().map(String::from));
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(Settings::parse(&json).unwrap(), s);
        assert_eq!((file_version(text), file_version(&json), file_version("{}")), (2, 3, 1));
    }

    #[test]
    fn broken_or_partial_files_do_not_panic() {
        assert!(Settings::parse("{ not json").is_err());
        let s = Settings::parse("{\"last_input_port\": \"kbd\", \"active_curve_tab\": 7}").unwrap();
        assert_eq!(s.last_input_port.as_deref(), Some("kbd"));
        assert_eq!(s.active_curve_tab, 1);
        assert_eq!(s.curves, CurveSet::default());
    }

    #[test]
    fn load_save_with_backup_of_old_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg").join("settings.json");
        // Нет файла — умолчания, без ошибки.
        assert_eq!(Settings::load(&path), (Settings::default(), None));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, include_str!("../tests/fixtures/legacy/settings.json")).unwrap();
        let (s, err) = Settings::load(&path);
        assert!(err.is_none());
        assert!(path.with_file_name("settings.v1.json").exists());
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path).0, s);
        // Формат 2 — своя копия, новый формат копий не делает.
        std::fs::write(&path, include_str!("../tests/fixtures/v2/settings.json")).unwrap();
        let (s2, err) = Settings::load(&path);
        assert!(err.is_none());
        assert!(path.with_file_name("settings.v2.json").exists());
        s2.save(&path).unwrap();
        std::fs::remove_file(path.with_file_name("settings.v2.json")).unwrap();
        Settings::load(&path);
        assert!(!path.with_file_name("settings.v3.json").exists());
        std::fs::write(&path, "garbage").unwrap();
        let (d, err) = Settings::load(&path);
        assert_eq!(d, Settings::default());
        assert!(err.is_some());
    }
}
