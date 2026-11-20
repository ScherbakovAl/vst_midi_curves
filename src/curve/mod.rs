//! Кривые velocity: форма ([`shape`]), таблицы потока MIDI ([`bank`]),
//! миграция старого формата ([`legacy`]).
//!
//! Copyright (c) 2025 Scherbakov Alexey <scherbakov.al@gmail.com>
//! Licensed under the MIT License

pub mod bank;
pub mod legacy;
pub mod shape;

pub use bank::{CurveBank, Slot, LUT_N};
pub use shape::{CurveShape, Knot};

/// Обе кривые и формат выхода — то, что хранится в проекте DAW (плагин) и в
/// `settings.json` (standalone).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CurveSet {
    pub note_on: CurveShape,
    pub note_off: CurveShape,
    /// Формат выхода: true — 14 бит (CC88 + нота), false — 7 бит.
    pub hi_res: bool,
}

impl Default for CurveSet {
    fn default() -> Self {
        Self { note_on: CurveShape::linear(), note_off: CurveShape::linear(), hi_res: false }
    }
}

impl CurveSet {
    pub fn shape(&self, slot: Slot) -> &CurveShape {
        match slot {
            Slot::NoteOn => &self.note_on,
            Slot::NoteOff => &self.note_off,
        }
    }

    pub fn shape_mut(&mut self, slot: Slot) -> &mut CurveShape {
        match slot {
            Slot::NoteOn => &mut self.note_on,
            Slot::NoteOff => &mut self.note_off,
        }
    }

    /// Починить формы из файла/проекта (см. [`CurveShape::normalize`]).
    pub fn normalize(&mut self) {
        self.note_on.normalize();
        self.note_off.normalize();
    }

    /// Залить обе таблицы и формат выхода в банк потока MIDI.
    pub fn apply(&self, bank: &CurveBank) {
        for slot in Slot::ALL {
            bank.store(slot, self.shape(slot));
        }
        bank.set_hi_res(self.hi_res);
    }
}
