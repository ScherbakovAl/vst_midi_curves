//! Кривые velocity: форма ([`shape`]), карта нот с морфингом ([`notemap`]),
//! таблицы потока MIDI ([`bank`]), миграция старого формата ([`legacy`]).
//!
//! Copyright (c) 2025 Scherbakov Alexey <scherbakov.al@gmail.com>
//! Licensed under the MIT License

pub mod bank;
pub mod legacy;
pub mod notemap;
pub mod shape;

pub use bank::{CurveBank, Slot, LUT_N};
pub use notemap::{Axis, KeyCurve, NoteMap};
pub use shape::{CurveShape, Knot};

/// Обе карты нот и формат выхода — то, что хранится в проекте DAW (плагин), в
/// `settings.json` (standalone) и в пресете «всё сразу».
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CurveSet {
    pub note_on: NoteMap,
    pub note_off: NoteMap,
    /// Формат выхода: true — 14 бит (CC88 + нота), false — 7 бит.
    pub hi_res: bool,
}

impl CurveSet {
    pub fn map(&self, slot: Slot) -> &NoteMap {
        match slot {
            Slot::NoteOn => &self.note_on,
            Slot::NoteOff => &self.note_off,
        }
    }

    pub fn map_mut(&mut self, slot: Slot) -> &mut NoteMap {
        match slot {
            Slot::NoteOn => &mut self.note_on,
            Slot::NoteOff => &mut self.note_off,
        }
    }

    /// Починить карты из файла/проекта (см. [`NoteMap::normalize`]).
    pub fn normalize(&mut self) {
        self.note_on.normalize();
        self.note_off.normalize();
    }

    /// Залить таблицы всех нот обеих кривых и формат выхода в банк потока MIDI.
    pub fn apply(&self, bank: &CurveBank) {
        for slot in Slot::ALL {
            bank.store_map(slot, self.map(slot), 0, (notemap::NOTES - 1) as u8);
        }
        bank.set_hi_res(self.hi_res);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Поле "curves" проекта DAW формата 2 (одна форма на кривую): каждая форма
    /// становится картой из одной ключевой — та же кривая на всех нотах.
    #[test]
    fn plugin_state_v2_reads_as_single_key_maps() {
        let text = include_str!("../../tests/fixtures/v2/plugin_curves.json");
        let old: serde_json::Value = serde_json::from_str(text).unwrap();
        let set: CurveSet = serde_json::from_str(text).unwrap();
        assert!(set.hi_res);
        for (map, key) in [(&set.note_on, "note_on"), (&set.note_off, "note_off")] {
            let shape: CurveShape = serde_json::from_value(old[key].clone()).unwrap();
            assert_eq!(map, &NoteMap::single(shape.clone()));
            for n in 0..notemap::NOTES as u8 {
                assert_eq!(map.shape_at(n), shape);
            }
        }
        let bank = CurveBank::new();
        set.apply(&bank);
        for n in [0u8, 21, 60, 108, 127] {
            assert!((bank.eval(Slot::NoteOn, n, 0.25) - set.note_on.keys[0].shape.eval(0.25)).abs() < 1e-5);
        }
    }

    #[test]
    fn set_roundtrips_in_format_3() {
        let mut set = CurveSet { hi_res: true, ..CurveSet::default() };
        let k = set.note_on.add_key(90);
        set.note_on.keys[k].shape.move_knot(1, 1.0, 0.4);
        set.note_on.set_lane_handle(Axis::Y, 1, k, false, [-0.05, 0.1], true);
        set.note_off.set_override(30, CurveShape::linear());
        let json = serde_json::to_string(&set).unwrap();
        assert_eq!(serde_json::from_str::<CurveSet>(&json).unwrap(), set);
    }
}
