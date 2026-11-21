//! Библиотека пресетов: встроенные формы + файлы в общей папке
//! `~/.config/vst_midi_curves/presets` (одна на плагин и standalone).
//!
//! Пресет — `<имя>.json` одного из двух видов:
//! - форма одной кривой: `{"version": 2, "shape": {...}}` (тот же `shape`, что
//!   у форм piano_sim); грузится в выбранную ноту;
//! - всё сразу: `{"version": 3, "set": {...}}` — карты нот Note On и Note Off
//!   и формат выхода ([`CurveSet`]); грузится целиком.
//!
//! Старые файлы (`control_points`, до 2026-11) читаются через точную миграцию
//! и на диске не переписываются. Папка читается только по запросу GUI — не в
//! конструкторе плагина (его хост вызывает и при сканировании).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::curve::legacy::{shape_from_legacy, LegacyPoint};
use crate::curve::{CurveSet, CurveShape, Knot};

/// Версия формата файла формы. 1 — старый формат без поля `version`.
pub const FILE_VERSION: u32 = 2;
/// Версия формата пресета «всё сразу».
pub const SET_FILE_VERSION: u32 = 3;

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

#[derive(Serialize, Deserialize)]
struct SetFile {
    version: u32,
    set: CurveSet,
}

#[derive(Deserialize)]
struct LegacyFile {
    control_points: Vec<LegacyPoint>,
}

/// Содержимое пресета.
#[derive(Clone, Debug, PartialEq)]
pub enum Preset {
    /// Форма одной кривой.
    Shape(CurveShape),
    /// Всё сразу: обе карты нот и формат выхода.
    Set(CurveSet),
}

/// Пресет из текста файла: «всё сразу», форма, старая форма (миграция).
/// Формы и карты чинятся.
pub fn parse(text: &str) -> Result<Preset, String> {
    if let Ok(f) = serde_json::from_str::<SetFile>(text) {
        let mut set = f.set;
        set.normalize();
        return Ok(Preset::Set(set));
    }
    let mut shape = match serde_json::from_str::<ShapeFile>(text) {
        Ok(f) => f.shape,
        Err(e) => match serde_json::from_str::<LegacyFile>(text) {
            Ok(old) => shape_from_legacy(&old.control_points),
            Err(_) => return Err(e.to_string()),
        },
    };
    shape.normalize();
    Ok(Preset::Shape(shape))
}

/// Пресет «всё сразу»? (Только по наличию поля `set` — для списка.)
fn is_set_file(text: &str) -> bool {
    #[derive(Deserialize)]
    struct Probe {
        set: Option<serde::de::IgnoredAny>,
    }
    serde_json::from_str::<Probe>(text).is_ok_and(|p| p.set.is_some())
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
    /// Пресет «всё сразу» (иначе — форма).
    pub set: bool,
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
    /// Файлы читаются (вид пресета для списка) — только по запросу GUI.
    pub fn scan(&self) -> Vec<Entry> {
        let mut files: Vec<(String, bool)> = self
            .dir
            .as_ref()
            .and_then(|d| std::fs::read_dir(d).ok())
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "json"))
                    .filter_map(|p| {
                        let name = p.file_stem()?.to_string_lossy().into_owned();
                        let set = std::fs::read_to_string(&p).is_ok_and(|t| is_set_file(&t));
                        Some((name, set))
                    })
                    .collect()
            })
            .unwrap_or_default();
        files.sort_by_key(|(n, _)| n.to_lowercase());
        let mut out: Vec<Entry> = builtins()
            .into_iter()
            .filter(|(n, _)| !files.iter().any(|(f, _)| f == n))
            .map(|(n, _)| Entry { name: n.to_string(), file: false, set: false })
            .collect();
        out.extend(files.into_iter().map(|(name, set)| Entry { name, file: true, set }));
        out
    }

    /// Пресет по имени: файл, иначе встроенная форма.
    pub fn load(&self, name: &str) -> Result<Preset, String> {
        if let Some(p) = self.path(name).filter(|p| p.exists()) {
            let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
            return parse(&text);
        }
        builtins()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, s)| Preset::Shape(s))
            .ok_or_else(|| format!("no preset «{name}»"))
    }

    /// Сохранить форму одной кривой.
    pub fn save(&self, name: &str, shape: &CurveShape) -> Result<(), String> {
        let file = ShapeFile { version: FILE_VERSION, shape: shape.clone(), description: String::new() };
        self.write(name, serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?)
    }

    /// Сохранить всё сразу: обе карты нот и формат выхода.
    pub fn save_set(&self, name: &str, set: &CurveSet) -> Result<(), String> {
        let file = SetFile { version: SET_FILE_VERSION, set: set.clone() };
        self.write(name, serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?)
    }

    fn write(&self, name: &str, json: String) -> Result<(), String> {
        if !name_ok(name) {
            return Err(format!("bad preset name «{name}»"));
        }
        let (dir, path) = match (&self.dir, self.path(name)) {
            (Some(d), Some(p)) => (d, p),
            _ => return Err("no config directory".into()),
        };
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
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
            let Preset::Shape(shape) = lib.load(name).unwrap() else { panic!("{name}: не форма") };
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
        assert_eq!(lib.load("My curve").unwrap(), Preset::Shape(s.clone()));
        // Файл с именем встроенной формы её замещает, в списке — одна строка.
        lib.save("Linear", &s).unwrap();
        let list = lib.scan();
        assert_eq!(list.iter().filter(|e| e.name == "Linear").count(), 1);
        assert!(list.iter().any(|e| e.name == "Linear" && e.file));
        assert_eq!(lib.load("Linear").unwrap(), Preset::Shape(s.clone()));
        lib.delete("Linear").unwrap();
        assert_eq!(lib.load("Linear").unwrap(), Preset::Shape(CurveShape::linear()));
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
        let Ok(Preset::Shape(shape)) = lib.load("P2") else { panic!("P2 не прочитан как форма") };
        assert_eq!(shape.knots.len(), 2);
        // Файл не переписан.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert!(parse("{\"garbage\": 1}").is_err());
    }

    /// Пресет «всё сразу»: обе карты нот и формат выхода — туда и обратно
    /// бит-в-бит, в списке помечен, форма рядом остаётся формой.
    #[test]
    fn whole_setup_presets_roundtrip() {
        let dir = tmp();
        let lib = Library::new(Some(dir.path().join("presets")));
        let mut set = CurveSet { hi_res: true, ..CurveSet::default() };
        let k = set.note_on.add_key(84);
        set.note_on.keys[k].shape.move_knot(1, 1.0, 0.6);
        set.note_off.set_override(40, CurveShape::linear());
        lib.save_set("Studio", &set).unwrap();
        lib.save("Curve", &CurveShape::linear()).unwrap();
        assert_eq!(lib.load("Studio").unwrap(), Preset::Set(set));
        let list = lib.scan();
        let flags: Vec<(bool, bool)> =
            ["Curve", "Studio", "Linear"].iter().map(|n| list.iter().find(|e| e.name == *n).map(|e| (e.file, e.set)).unwrap()).collect();
        assert_eq!(flags, vec![(true, false), (true, true), (false, false)]);
        // Старый формат пресета-формы читается и рядом с новыми.
        assert!(matches!(parse(include_str!("../tests/fixtures/legacy/pianoteq.json")), Ok(Preset::Shape(_))));
    }

    #[test]
    fn missing_dir_lists_builtins_only() {
        let lib = Library::new(Some(PathBuf::from("/nonexistent/vst_midi_curves")));
        let list = lib.scan();
        assert_eq!(list.len(), builtins().len());
        assert!(list.iter().all(|e| !e.file));
    }
}
