//! Банк кривых для потока MIDI (аудиопоток плагина, колбэк midir в standalone):
//! таблицы кривых Note On / Note Off — своя на каждую из 128 нот — в атомиках,
//! флаг формата выхода и маркеры последней ноты. Паттерн `VelCurveBank` из
//! piano_sim.
//!
//! Поток MIDI только читает (`eval`, `hi_res`) и пишет маркеры (`note`);
//! GUI переписывает таблицы при правке (`store*`). Блокировок нет, всё —
//! `Relaxed`: межпеременных инвариантов нет. Пока GUI переписывает таблицу
//! во время драга, нота может взять смесь соседних старых и новых точек — это
//! промежуточная форма той же правки, выход всё равно в [0, 1].
//!
//! Память: 2 × 128 × 1025 × 4 Б ≈ 1 МБ на экземпляр; событие читает две
//! соседние точки таблицы своей ноты (первое касание ноты — промах кэша).

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};

use super::notemap::{NoteMap, NOTES};
use super::shape::CurveShape;

/// Точек таблицы кривой по velocity (шаг 1/1024, между точками — линейно).
/// Ошибка линейной интерполяции ≤ h²/8 · max|y''|, h = 1/1024; проверяется
/// тестом `lut_matches_exact_eval` по всем 16384 кодам.
pub const LUT_N: usize = 1025;

/// Какая кривая.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    NoteOn = 0,
    NoteOff = 1,
}

impl Slot {
    pub const ALL: [Slot; 2] = [Slot::NoteOn, Slot::NoteOff];

    #[inline]
    pub fn idx(self) -> usize {
        self as usize
    }
}

pub struct CurveBank {
    /// y (0…1) по равномерной сетке x, биты f32: таблица (кривая, нота)
    /// начинается с (slot·128 + note)·LUT_N. В куче: 1 МБ на стеке не собрать.
    lut: Box<[AtomicU32]>,
    /// Формат выхода: true — 14 бит (CC88 + нота), false — 7 бит.
    hi_res: AtomicBool,
    /// Вход x (0…1) последней ноты каждой кривой, биты f32; NaN — нот не было.
    last_x: [AtomicU32; 2],
    /// Номер последней ноты каждой кривой.
    last_note: [AtomicU8; 2],
    /// Счётчик обработанных нот (GUI перерисовывает маркер, когда он меняется).
    notes: AtomicU64,
    /// Счётчик заливок таблиц.
    edits: AtomicU64,
}

impl Default for CurveBank {
    fn default() -> Self {
        Self::new()
    }
}

/// Начало таблицы (кривая, нота); нота зажата маской — индекс всегда в банке.
#[inline]
fn base(slot: Slot, note: u8) -> usize {
    (slot.idx() * NOTES + (note & 0x7F) as usize) * LUT_N
}

impl CurveBank {
    /// Все кривые всех нот — диагональ, выход 7 бит.
    pub fn new() -> Self {
        let bank = Self {
            lut: (0..2 * NOTES * LUT_N).map(|_| AtomicU32::new(0)).collect(),
            hi_res: AtomicBool::new(false),
            last_x: std::array::from_fn(|_| AtomicU32::new(f32::NAN.to_bits())),
            last_note: std::array::from_fn(|_| AtomicU8::new(0)),
            notes: AtomicU64::new(0),
            edits: AtomicU64::new(0),
        };
        let lin = CurveShape::linear();
        for s in Slot::ALL {
            bank.store(s, &lin);
        }
        bank
    }

    /// Записать готовую таблицу ноты.
    fn write(&self, slot: Slot, note: u8, ys: &[f32; LUT_N]) {
        let b = base(slot, note);
        for (a, y) in self.lut[b..b + LUT_N].iter().zip(ys) {
            a.store(y.to_bits(), Ordering::Relaxed);
        }
    }

    /// GUI / главный поток: одна форма на все ноты кривой.
    pub fn store(&self, slot: Slot, shape: &CurveShape) {
        let mut ys = [0.0f32; LUT_N];
        shape.fill_lut(&mut ys);
        for n in 0..NOTES as u8 {
            self.write(slot, n, &ys);
        }
        self.edits.fetch_add(1, Ordering::Relaxed);
    }

    /// GUI / главный поток: таблица одной ноты ([`CurveShape::fill_lut`]).
    pub fn store_note(&self, slot: Slot, note: u8, shape: &CurveShape) {
        let mut ys = [0.0f32; LUT_N];
        shape.fill_lut(&mut ys);
        self.write(slot, note, &ys);
        self.edits.fetch_add(1, Ordering::Relaxed);
    }

    /// GUI / главный поток: таблицы нот `lo..=hi` по карте. Подряд идущие
    /// одинаковые формы (ниже первой / выше последней ключевой, одна
    /// ключевая) считаются один раз.
    pub fn store_map(&self, slot: Slot, map: &NoteMap, lo: u8, hi: u8) {
        let mut ys = [0.0f32; LUT_N];
        let mut prev: Option<CurveShape> = None;
        for n in lo..=hi.min((NOTES - 1) as u8) {
            let s = map.shape_at(n);
            if prev.as_ref() != Some(&s) {
                s.fill_lut(&mut ys);
                prev = Some(s);
            }
            self.write(slot, n, &ys);
        }
        self.edits.fetch_add(1, Ordering::Relaxed);
    }

    /// Поток MIDI: y(x) ноты `note` по её таблице, O(1). x вне [0, 1] и NaN
    /// зажимаются.
    ///
    /// f = x·(N−1), j = min(⌊f⌋, N−2), w = f − j, y = L[j] + (L[j+1] − L[j])·w.
    #[inline]
    pub fn eval(&self, slot: Slot, note: u8, x: f32) -> f32 {
        let b = base(slot, note);
        let lut = &self.lut[b..b + LUT_N];
        let x = if x > 0.0 { x.min(1.0) } else { 0.0 };
        let f = x * (LUT_N - 1) as f32;
        let j = (f as usize).min(LUT_N - 2);
        let w = f - j as f32;
        let ya = f32::from_bits(lut[j].load(Ordering::Relaxed));
        let yb = f32::from_bits(lut[j + 1].load(Ordering::Relaxed));
        ya + (yb - ya) * w
    }

    #[inline]
    pub fn hi_res(&self) -> bool {
        self.hi_res.load(Ordering::Relaxed)
    }

    pub fn set_hi_res(&self, on: bool) {
        self.hi_res.store(on, Ordering::Relaxed);
    }

    /// Поток MIDI: запомнить ноту и её вход (маркер в редакторе).
    #[inline]
    pub fn note(&self, slot: Slot, note: u8, x: f32) {
        self.last_note[slot.idx()].store(note & 0x7F, Ordering::Relaxed);
        self.last_x[slot.idx()].store(x.to_bits(), Ordering::Relaxed);
        self.notes.fetch_add(1, Ordering::Relaxed);
    }

    /// Вход последней ноты кривой `slot` (0…1), если ноты были.
    pub fn last_x(&self, slot: Slot) -> Option<f32> {
        let v = f32::from_bits(self.last_x[slot.idx()].load(Ordering::Relaxed));
        v.is_finite().then_some(v)
    }

    /// Номер последней ноты кривой `slot`, если ноты были.
    pub fn last_note(&self, slot: Slot) -> Option<u8> {
        self.last_x(slot).map(|_| self.last_note[slot.idx()].load(Ordering::Relaxed))
    }

    /// Сколько нот обработано (для перерисовки маркера и счётчика в GUI).
    pub fn notes(&self) -> u64 {
        self.notes.load(Ordering::Relaxed)
    }

    pub fn edits(&self) -> u64 {
        self.edits.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::curve::legacy::{shape_from_legacy, LegacyPoint};

    /// Наибольшая ошибка таблицы против точного `eval` по всем 16384 кодам, в
    /// младших разрядах 14 бит.
    fn lut_error(shape: &CurveShape) -> f32 {
        let bank = CurveBank::new();
        bank.store_note(Slot::NoteOn, 64, shape);
        (0..=16383u32)
            .map(|code| {
                let x = code as f32 / 16383.0;
                (bank.eval(Slot::NoteOn, 64, x) - shape.eval(x)).abs() * 16383.0
            })
            .fold(0.0, f32::max)
    }

    /// Реальные формы (пресеты пользователя P2/p2/pianoteq, S-кривая, узел с
    /// изломом): меньше 0.5 младшего разряда 14 бит. Замер 2026-11-20:
    /// P2 0.29, p2 0.25, pianoteq 0.08.
    #[test]
    fn lut_matches_exact_eval() {
        let legacy = |v: &[(f32, f32)]| {
            shape_from_legacy(&v.iter().map(|&(x, y)| LegacyPoint::new(x, y)).collect::<Vec<_>>())
        };
        let mut s = CurveShape::linear();
        s.set_handle(0, true, [0.5, 0.0], false);
        s.set_handle(1, false, [0.5, 1.0], false);
        let mut corner = CurveShape::linear();
        let i = corner.insert_at(0.5).unwrap();
        corner.move_knot(i, 0.5, 0.9);
        let shapes = [
            CurveShape::linear(),
            legacy(&[(0.0, 0.0), (6.3313966, 19.339388), (3.0616071, 79.562416), (127.0, 127.0)]),
            legacy(&[(0.0, 0.0), (7.765639, 2.9035265), (1.3465401, 67.002975), (127.0, 127.0)]),
            legacy(&[(0.0, 0.0), (25.518707, 88.82972), (127.0, 127.0)]),
            s,
            corner,
        ];
        for shape in shapes {
            let e = lut_error(&shape);
            assert!(e < 0.5, "ошибка таблицы {e} младшего разряда, форма {shape:?}");
        }
    }

    /// Предельно крутая форма (наклон 30 у нуля): ошибка линейной интерполяции
    /// h²/8·max|y''| растёт, но остаётся меньше 8 младших разрядов 14 бит
    /// (0.05 % шкалы, 1/16 шага 7 бит). Замер 2026-11-20: 6.3.
    #[test]
    fn lut_error_is_bounded_for_extreme_shapes() {
        let mut steep = CurveShape::linear();
        steep.knots[0].hout = [0.02, 0.6];
        steep.knots[1].hin = [-0.9, -0.05];
        let e = lut_error(&steep);
        assert!(e < 8.0, "ошибка таблицы {e}");
    }

    #[test]
    fn eval_clamps_input() {
        let bank = CurveBank::new();
        assert!(bank.eval(Slot::NoteOff, 0, -3.0) < 1e-6);
        assert_eq!(bank.eval(Slot::NoteOff, 0, -3.0), bank.eval(Slot::NoteOff, 0, f32::NAN));
        assert!((bank.eval(Slot::NoteOff, 127, 7.0) - 1.0).abs() < 1e-6);
        // Нота за 127 (битый вход) — маска, не выход за банк.
        assert_eq!(bank.eval(Slot::NoteOff, 200, 0.5), bank.eval(Slot::NoteOff, 200 & 0x7F, 0.5));
    }

    /// Таблицы нот независимы: своя форма у ноты и у кривой, карта заливается
    /// по нотам, маркер помнит ноту.
    #[test]
    fn tables_per_note_and_map() {
        let bank = CurveBank::new();
        let mut low = CurveShape::linear();
        low.move_knot(1, 1.0, 0.5);
        bank.store_note(Slot::NoteOn, 40, &low);
        assert!((bank.eval(Slot::NoteOn, 40, 1.0) - 0.5).abs() < 1e-6);
        assert!((bank.eval(Slot::NoteOn, 41, 1.0) - 1.0).abs() < 1e-6);
        assert!((bank.eval(Slot::NoteOff, 40, 1.0) - 1.0).abs() < 1e-6);
        // Карта: ключевые 30 (y(1) = 0.5) и 90 (диагональ) — линейный морф.
        let mut map = NoteMap::single(low);
        map.keys[0].note = 30;
        let k = map.add_key(90);
        map.keys[k].shape = CurveShape::linear();
        bank.store_map(Slot::NoteOff, &map, 0, 127);
        for n in 0..NOTES as u8 {
            let t = ((n as f32 - 30.0) / 60.0).clamp(0.0, 1.0);
            let want = 0.5 + 0.5 * t;
            assert!((bank.eval(Slot::NoteOff, n, 1.0) - want).abs() < 1e-5, "нота {n}");
            let s = map.shape_at(n);
            for x in [0.1f32, 0.37, 0.8] {
                assert!((bank.eval(Slot::NoteOff, n, x) - s.eval(x)).abs() < 1e-5, "нота {n}, x {x}");
            }
        }
        assert_eq!((bank.last_note(Slot::NoteOn), bank.last_x(Slot::NoteOn)), (None, None));
        bank.note(Slot::NoteOn, 61, 0.25);
        assert_eq!((bank.last_note(Slot::NoteOn), bank.last_x(Slot::NoteOn)), (Some(61), Some(0.25)));
    }
}

