//! Библиотека форм кривых: встроенные + файлы в общей папке
//! `~/.config/vst_midi_curves/presets` (одна на плагин и standalone).
//!
//! Файл формы — `<имя>.json`, формат `{"version": 2, "shape": {...}}` (тот же
//! `shape`, что у форм piano_sim). Старые файлы (`control_points`, до
//! 2026-11) читаются через точную миграцию и на диске не переписываются.
//! Папка читается только по запросу GUI — не в конструкторе плагина (его
//! хост вызывает и при сканировании).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::curve::legacy::{shape_from_legacy, LegacyPoint};
use crate::curve::{CurveShape, Knot};

/// Версия формата файла формы. 1 — старый формат без поля `version`.
pub const FILE_VERSION: u32 = 2;

/// Папка библиотеки по умолчанию; `None` — у системы нет папки настроек.
pub fn default_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("vst_midi_curves").join("presets"))
}

#[derive(Serialize, Deserialize)]
struct ShapeFile {
    version: u32,
    shape: CurveShape,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
}

#[derive(Deserialize)]
struct LegacyFile {
    control_points: Vec<LegacyPoint>,
}

/// Форма из текста файла: новый формат или старый (миграция). Форма чинится.
pub fn parse(text: &str) -> Result<CurveShape, String> {
    let mut shape = match serde_json::from_str::<ShapeFile>(text) {
        Ok(f) => f.shape,
        Err(e) => match serde_json::from_str::<LegacyFile>(text) {
            Ok(old) => shape_from_legacy(&old.control_points),
            Err(_) => return Err(e.to_string()),
        },
    };
    shape.normalize();
    Ok(shape)
}

/// Имя формы годится в имя файла: непустое, без разделителей пути, ведущей
/// точки и символов, запрещённых в Windows.
pub fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
}

/// Встроенная форма из двух узлов: один кубический Безье с ручками в точки
/// `p1`, `p2` (единицы velocity 0…127) — те же формы, что звучали у
/// встроенных пресетов до 2026-11.
fn two_knot(p1: [f32; 2], p2: [f32; 2]) -> CurveShape {
    const S: f32 = 127.0;
    CurveShape {
        knots: vec![
            Knot { x: 0.0, y: 0.0, hin: [0.0, 0.0], hout: [p1[0] / S, p1[1] / S], smooth: true },
            Knot { x: 1.0, y: 1.0, hin: [p2[0] / S - 1.0, p2[1] / S - 1.0], hout: [0.0, 0.0], smooth: true },
        ],
    }
}

/// Встроенные формы (в списке первыми; файл с тем же именем их заменяет).
pub fn builtins() -> Vec<(&'static str, CurveShape)> {
    vec![
        ("Linear", CurveShape::linear()),
        ("Soft S-Curve", two_knot([42.0, 32.0], [85.0, 95.0])),
        ("Exponential", two_knot([25.0, 12.0], [63.0, 38.0])),
        ("Inverse Exponential", two_knot([64.0, 89.0], [102.0, 115.0])),
        ("Sigmoid", two_knot([32.0, 16.0], [95.0, 111.0])),
        ("Hard Step", two_knot([64.0, 0.0], [64.0, 127.0])),
        ("Gentle Curve", two_knot([32.0, 20.0], [95.0, 107.0])),
        ("Aggressive Curve", two_knot([48.0, 8.0], [80.0, 80.0])),
    ]
}

/// Строка списка библиотеки.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    /// Есть файл (его можно удалить); иначе — только встроенная.
    pub file: bool,
}

/// Папка библиотеки. Все операции — синхронный файловый ввод-вывод: только
/// из GUI, по действию пользователя.
#[derive(Clone, Debug)]
pub struct Library {
    dir: Option<PathBuf>,
}

impl Library {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    fn path(&self, name: &str) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(format!("{name}.json")))
    }

    /// Встроенные (кроме замещённых файлами), затем файлы — по алфавиту.
    pub fn scan(&self) -> Vec<Entry> {
        let mut files: Vec<String> = self
            .dir
            .as_ref()
            .and_then(|d| std::fs::read_dir(d).ok())
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "json"))
                    .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
                    .collect()
            })
            .unwrap_or_default();
        files.sort_by_key(|n| n.to_lowercase());
        let mut out: Vec<Entry> = builtins()
            .into_iter()
            .filter(|(n, _)| !files.iter().any(|f| f == n))
            .map(|(n, _)| Entry { name: n.to_string(), file: false })
            .collect();
        out.extend(files.into_iter().map(|name| Entry { name, file: true }));
        out
    }

    /// Форма по имени: файл, иначе встроенная.
    pub fn load(&self, name: &str) -> Result<CurveShape, String> {
        if let Some(p) = self.path(name).filter(|p| p.exists()) {
            let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
            return parse(&text);
        }
        builtins()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, s)| s)
            .ok_or_else(|| format!("no preset «{name}»"))
    }

    pub fn save(&self, name: &str, shape: &CurveShape) -> Result<(), String> {
        if !name_ok(name) {
            return Err(format!("bad preset name «{name}»"));
        }
        let (dir, path) = match (&self.dir, self.path(name)) {
            (Some(d), Some(p)) => (d, p),
            _ => return Err("no config directory".into()),
        };
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let file = ShapeFile { version: FILE_VERSION, shape: shape.clone(), description: String::new() };
        let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn delete(&self, name: &str) -> Result<(), String> {
        let path = self.path(name).ok_or("no config directory")?;
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::legacy::legacy_eval;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn builtins_sound_like_the_old_ones() {
        // Те же точки, что у старых встроенных пресетов: форма совпадает.
        let old: [(&str, [(f32, f32); 4]); 7] = [
            ("Soft S-Curve", [(0.0, 0.0), (42.0, 32.0), (85.0, 95.0), (127.0, 127.0)]),
            ("Exponential", [(0.0, 0.0), (25.0, 12.0), (63.0, 38.0), (127.0, 127.0)]),
            ("Inverse Exponential", [(0.0, 0.0), (64.0, 89.0), (102.0, 115.0), (127.0, 127.0)]),
            ("Sigmoid", [(0.0, 0.0), (32.0, 16.0), (95.0, 111.0), (127.0, 127.0)]),
            ("Hard Step", [(0.0, 0.0), (64.0, 0.0), (64.0, 127.0), (127.0, 127.0)]),
            ("Gentle Curve", [(0.0, 0.0), (32.0, 20.0), (95.0, 107.0), (127.0, 127.0)]),
            ("Aggressive Curve", [(0.0, 0.0), (48.0, 8.0), (80.0, 80.0), (127.0, 127.0)]),
        ];
        let lib = Library::new(None);
        for (name, pts) in old {
            let shape = lib.load(name).unwrap();
            let legacy: Vec<LegacyPoint> = pts.iter().map(|&(x, y)| LegacyPoint::new(x, y)).collect();
            for j in 0..=127 * 4 {
                let x = j as f32 / 4.0;
                let d = (shape.eval(x / 127.0) * 127.0 - legacy_eval(&legacy, x)).abs();
                assert!(d < 0.01, "{name}: x={x} расхождение {d}");
            }
        }
    }

    #[test]
    fn save_load_roundtrip_and_shadowing() {
        let dir = tmp();
        let lib = Library::new(Some(dir.path().join("presets")));
        let mut s = CurveShape::linear();
        s.insert_at(0.3).unwrap();
        s.move_knot(1, 0.3, 0.7);
        lib.save("My curve", &s).unwrap();
        assert_eq!(lib.load("My curve").unwrap(), s);
        // Файл с именем встроенной формы её замещает, в списке — одна строка.
        lib.save("Linear", &s).unwrap();
        let list = lib.scan();
        assert_eq!(list.iter().filter(|e| e.name == "Linear").count(), 1);
        assert!(list.iter().any(|e| e.name == "Linear" && e.file));
        assert_eq!(lib.load("Linear").unwrap(), s);
        lib.delete("Linear").unwrap();
        assert_eq!(lib.load("Linear").unwrap(), CurveShape::linear());
        assert!(lib.save("../evil", &s).is_err());
        assert!(lib.save(".hidden", &s).is_err());
    }

    #[test]
    fn legacy_files_are_read_through_migration() {
        let dir = tmp();
        let path = dir.path().join("P2.json");
        let text = include_str!("../tests/fixtures/legacy/p2_upper.json");
        std::fs::write(&path, text).unwrap();
        let lib = Library::new(Some(dir.path().to_path_buf()));
        let shape = lib.load("P2").unwrap();
        assert_eq!(shape.knots.len(), 2);
        // Файл не переписан.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert!(parse("{\"garbage\": 1}").is_err());
    }

    #[test]
    fn missing_dir_lists_builtins_only() {
        let lib = Library::new(Some(PathBuf::from("/nonexistent/vst_midi_curves")));
        let list = lib.scan();
        assert_eq!(list.len(), builtins().len());
        assert!(list.iter().all(|e| !e.file));
    }
}
