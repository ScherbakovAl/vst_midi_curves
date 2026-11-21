//! Публичное API крейта целиком, как им пользуются плагин и standalone:
//! библиотека форм и настройки во временной папке (реальный `~/.config` не
//! трогается), заливка кривых в банк, поток байтов MIDI через кривую.

use vst_midi_curves::curve::{CurveBank, CurveSet, CurveShape, NoteMap, Slot};
use vst_midi_curves::library::Library;
use vst_midi_curves::settings::Settings;
use vst_midi_curves::velocity::{process_midi, Prefix};

#[test]
fn preset_to_settings_to_midi_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::new(Some(dir.path().join("presets")));

    // Форма: внутренний узел поднят — середина шкалы громче.
    let mut louder = CurveShape::linear();
    let i = louder.insert_at(0.5).unwrap();
    louder.move_knot(i, 0.5, 0.75);
    lib.save("Louder", &louder).unwrap();
    assert!(lib.scan().iter().any(|e| e.name == "Louder" && e.file));

    // Настройки standalone: кривая из библиотеки, выход 14 бит.
    let set = CurveSet { note_on: lib.load("Louder").unwrap(), hi_res: true, ..CurveSet::default() };
    let path = dir.path().join("settings.json");
    let settings = Settings { curves: set, last_input_port: Some("kbd".into()), ..Settings::default() };
    settings.save(&path).unwrap();
    let (loaded, err) = Settings::load(&path);
    assert!(err.is_none());
    assert_eq!(loaded, settings);

    // Поток MIDI: CC88 + Note On середины шкалы выходит громче, Note Off —
    // по диагонали без изменений.
    let bank = CurveBank::new();
    loaded.curves.apply(&bank);
    let mut prefix = Prefix::new();
    let mut out: Vec<Vec<u8>> = Vec::new();
    for m in [[0xB0, 88, 0], [0x90, 60, 64], [0xB0, 88, 5], [0x80, 60, 30]] {
        process_midi(&bank, &mut prefix, &m, |o| out.push(o.to_vec()));
    }
    assert_eq!(out.len(), 4);
    assert_eq!(out[1][0], 0x90);
    assert!(out[1][2] > 64, "Note On середины не стал громче: {:?}", out[1]);
    assert_eq!(out[2..], [vec![0xB0, 88, 5], vec![0x80, 60, 30]]);
    assert_eq!(bank.last_x(Slot::NoteOff), Some((30.0 * 128.0 + 5.0) / 16383.0));
}

/// Карта нот: тихая форма (y = x/2) в ноте 30, диагональ в 90 — одна и та же velocity
/// звучит по-разному в разных нотах, между ключевыми — плавно.
#[test]
fn different_notes_follow_their_curves() {
    // Прямая y = x/2 (ручки по трети хорды).
    let mut quiet = CurveShape::linear();
    quiet.move_knot(1, 1.0, 0.5);
    quiet.knots[0].hout = [1.0 / 3.0, 1.0 / 6.0];
    quiet.knots[1].hin = [-1.0 / 3.0, -1.0 / 6.0];
    let mut map = NoteMap::single(quiet);
    map.keys[0].note = 30;
    let k = map.add_key(90);
    map.keys[k].shape = CurveShape::linear();
    let bank = CurveBank::new();
    bank.store_map(Slot::NoteOn, &map, 0, 127);
    let mut prefix = Prefix::new();
    let mut vel = |note: u8| {
        let mut v = 0;
        process_midi(&bank, &mut prefix, &[0x90, note, 100], |o| v = o[2]);
        v
    };
    let (low, mid, high) = (vel(24), vel(60), vel(100));
    assert_eq!((low, high), (50, 100), "крайние ключевые держатся до края клавиатуры");
    assert_eq!(mid, 75, "середина между ключевыми — середина морфа");
    assert_eq!(bank.last_note(Slot::NoteOn), Some(100));
}
