//! Velocity ноты через кривую: декодирование входа (7 бит или 14 бит с
//! префиксом CC88), кодирование выхода, префикс по каналам. Общая часть пути
//! MIDI плагина и standalone.
//!
//! Всё здесь — для потока MIDI: без блокировок, аллокаций и системных вызовов,
//! O(1) на событие (см. §2.1 `.work/AUDIT_PLAN.md`).
//!
//! CC88 — High Resolution Velocity Prefix (MIDI CA-031): младшие 7 бит (LL)
//! 14-битной velocity СЛЕДУЮЩЕЙ ноты канала; старшие 7 бит (HH) — data2 самой
//! ноты. Вход ноты, x ∈ [0, 1]:
//!
//!   x = (128·HH + LL) / 16383, если перед нотой был CC88 этого канала;
//!   x = HH / 127 иначе.
//!
//! 16383 = 2¹⁴ − 1 (наибольший 14-битный код), 127 = 2⁷ − 1. Обе шкалы
//! совпадают в точках HH/127 = 129·HH/16383 (16383 = 127·129).

use crate::curve::{CurveBank, Slot};

/// Номер контроллера префикса high-res velocity.
pub const CC_HIRES_VELOCITY: u8 = 88;
/// Наибольший 14-битный код velocity.
pub const V14_MAX: u16 = 16383;

/// Вход ноты 0…1 из HH (data2 ноты) и LL (значение CC88, если был).
#[inline]
pub fn decode(hh: u8, ll: Option<u8>) -> f32 {
    match ll {
        Some(ll) => ((((hh & 0x7F) as u32) << 7) | (ll & 0x7F) as u32) as f32 / V14_MAX as f32,
        None => (hh & 0x7F) as f32 / 127.0,
    }
}

/// Выход кривой y (0…1) → 7 бит. У Note On не меньше 1: velocity 0 — это
/// Note Off.
#[inline]
pub fn encode7(y: f32, note_on: bool) -> u8 {
    let v = (y.clamp(0.0, 1.0) * 127.0).round() as u8;
    if note_on {
        v.max(1)
    } else {
        v
    }
}

/// Выход кривой y (0…1) → 14 бит: (HH, LL), v14 = round(y·16383),
/// HH = v14 >> 7, LL = v14 & 0x7F. У Note On v14 ≥ 128 (HH ≥ 1).
#[inline]
pub fn encode14(y: f32, note_on: bool) -> (u8, u8) {
    let mut v = (y.clamp(0.0, 1.0) * V14_MAX as f32).round() as u16;
    if note_on {
        v = v.max(128);
    }
    ((v >> 7) as u8, (v & 0x7F) as u8)
}

/// Ожидающие префиксы CC88 — по слоту на каждый из 16 каналов. Ставится CC88,
/// расходуется следующей нотой (On/Off) того же канала; новый CC88 заменяет
/// старый. Без таймеров: офлайн-рендер даёт то же, что реальное время.
/// Владеет только поток MIDI.
#[derive(Clone, Debug)]
pub struct Prefix([u8; 16]);

const NO_PREFIX: u8 = 0x80;

impl Default for Prefix {
    fn default() -> Self {
        Self::new()
    }
}

impl Prefix {
    pub const fn new() -> Self {
        Self([NO_PREFIX; 16])
    }

    #[inline]
    pub fn set(&mut self, channel: u8, ll: u8) {
        self.0[(channel & 0x0F) as usize] = ll & 0x7F;
    }

    #[inline]
    pub fn take(&mut self, channel: u8) -> Option<u8> {
        let slot = &mut self.0[(channel & 0x0F) as usize];
        let v = std::mem::replace(slot, NO_PREFIX);
        (v != NO_PREFIX).then_some(v)
    }
}

/// Velocity ноты на выходе: `ll` — значение CC88, который нужно отправить
/// ПЕРЕД нотой (только в формате hi-res), `vel` — data2 ноты.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteOut {
    pub ll: Option<u8>,
    pub vel: u8,
}

/// Нота с входом `x` (0…1) через кривую `slot`; формат выхода — флаг банка.
/// Отмечает маркер последней ноты.
#[inline]
pub fn map_note(bank: &CurveBank, slot: Slot, x: f32) -> NoteOut {
    let y = bank.eval(slot, x);
    bank.note(slot, x);
    let note_on = slot == Slot::NoteOn;
    if bank.hi_res() {
        let (hh, ll) = encode14(y, note_on);
        NoteOut { ll: Some(ll), vel: hh }
    } else {
        NoteOut { ll: None, vel: encode7(y, note_on) }
    }
}

/// Одно входящее MIDI-сообщение (полное, со статусом) → 0…2 исходящих через
/// `emit` (буферы на стеке). CC88 поглощается (он относится к старой
/// velocity); Note On/Off идут через кривую, в hi-res — с новым CC88 перед
/// нотой; Note On с velocity 0 (= Note Off) расходует префикс и уходит как
/// есть; всё прочее (CC, aftertouch, pitch bend, SysEx, clock…) — без
/// изменений.
#[inline]
pub fn process_midi(bank: &CurveBank, prefix: &mut Prefix, msg: &[u8], mut emit: impl FnMut(&[u8])) {
    if let [status, data1, data2] = *msg {
        let ch = status & 0x0F;
        match status & 0xF0 {
            0xB0 if data1 == CC_HIRES_VELOCITY => {
                prefix.set(ch, data2);
                return;
            }
            0x90 if data2 == 0 => {
                prefix.take(ch);
            }
            kind @ (0x80 | 0x90) => {
                let slot = if kind == 0x90 { Slot::NoteOn } else { Slot::NoteOff };
                let x = decode(data2, prefix.take(ch));
                let out = map_note(bank, slot, x);
                if let Some(ll) = out.ll {
                    emit(&[0xB0 | ch, CC_HIRES_VELOCITY, ll]);
                }
                emit(&[status, data1, out.vel]);
                return;
            }
            _ => {}
        }
    }
    emit(msg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::CurveShape;

    fn run(bank: &CurveBank, prefix: &mut Prefix, msgs: &[&[u8]]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for m in msgs {
            process_midi(bank, prefix, m, |o| out.push(o.to_vec()));
        }
        out
    }

    #[test]
    fn diagonal_is_identity_on_all_codes() {
        let bank = CurveBank::new();
        bank.set_hi_res(true);
        for code in 0..=V14_MAX {
            let (hh, ll) = ((code >> 7) as u8, (code & 0x7F) as u8);
            for slot in Slot::ALL {
                let out = map_note(&bank, slot, decode(hh, Some(ll)));
                let expect = if slot == Slot::NoteOn && code < 128 { (1, 0) } else { (hh, ll) };
                assert_eq!((out.vel, out.ll.unwrap()), expect, "код {code}, {slot:?}");
            }
        }
        bank.set_hi_res(false);
        for v in 0..=127u8 {
            assert_eq!(map_note(&bank, Slot::NoteOn, decode(v, None)).vel, v.max(1));
            assert_eq!(map_note(&bank, Slot::NoteOff, decode(v, None)), NoteOut { ll: None, vel: v });
        }
    }

    #[test]
    fn seven_bit_input_spans_full_14_bit_scale() {
        // Нота без префикса при выходе hi-res: HH/127 → 129·HH, т.е. LL = HH.
        let bank = CurveBank::new();
        bank.set_hi_res(true);
        for hh in 1..=127u8 {
            assert_eq!(map_note(&bank, Slot::NoteOn, decode(hh, None)), NoteOut { ll: Some(hh), vel: hh });
        }
    }

    #[test]
    fn boundaries() {
        assert_eq!(decode(0, Some(0)), 0.0);
        assert_eq!(decode(127, Some(127)), 1.0);
        assert_eq!(decode(127, None), 1.0);
        assert_eq!(encode14(0.0, false), (0, 0));
        assert_eq!(encode14(8191.0 / 16383.0, false), (63, 127));
        assert_eq!(encode14(1.0, false), (127, 127));
        assert_eq!(encode14(1.5, true), (127, 127));
        assert_eq!(encode14(f32::NAN, true), (1, 0));
        assert_eq!(encode7(0.0, true), 1);
        assert_eq!(encode7(0.0, false), 0);
        assert_eq!(encode7(1.0, false), 127);
    }

    #[test]
    fn note_on_never_becomes_note_off() {
        // Кривая, прижатая к нулю: Note On всё равно с velocity ≥ 1 (HH ≥ 1).
        let bank = CurveBank::new();
        let mut zero = CurveShape::linear();
        zero.move_knot(1, 1.0, 0.0);
        bank.store(Slot::NoteOn, &zero);
        for hi in [false, true] {
            bank.set_hi_res(hi);
            for v in 1..=127u8 {
                assert!(map_note(&bank, Slot::NoteOn, decode(v, Some(5))).vel >= 1);
            }
        }
    }

    #[test]
    fn prefix_is_consumed_by_one_note_of_its_channel() {
        let bank = CurveBank::new();
        bank.set_hi_res(true);
        let mut p = Prefix::new();
        let out = run(
            &bank,
            &mut p,
            &[
                &[0xB0, 88, 37],  // префикс канала 0
                &[0x91, 60, 64],  // нота канала 1 — префикс канала 0 не трогает
                &[0x90, 60, 64],  // расходует префикс: 14 бит (64, 37)
                &[0x80, 60, 10],  // префикса уже нет: 7 бит → LL = HH
                &[0xB0, 64, 127], // педаль проходит как есть
            ],
        );
        assert_eq!(
            out,
            vec![
                vec![0xB1, 88, 64],
                vec![0x91, 60, 64],
                vec![0xB0, 88, 37],
                vec![0x90, 60, 64],
                vec![0xB0, 88, 10],
                vec![0x80, 60, 10],
                vec![0xB0, 64, 127],
            ]
        );
    }

    #[test]
    fn other_messages_pass_and_cc88_is_swallowed() {
        let bank = CurveBank::new();
        let mut p = Prefix::new();
        let sysex: &[u8] = &[0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7];
        let msgs: [&[u8]; 6] = [&[0xF8], &[0xA0, 60, 20], &[0xD0, 30], &[0xE0, 0, 64], sysex, &[0xB3, 88, 1]];
        let out = run(&bank, &mut p, &msgs);
        let expect: Vec<Vec<u8>> = msgs[..5].iter().map(|m| m.to_vec()).collect();
        assert_eq!(out, expect);
        // CC88 канала 3 ушёл в префикс, не на выход.
        assert_eq!(p.take(3), Some(1));
    }

    #[test]
    fn note_on_velocity_zero_passes_unchanged_and_eats_prefix() {
        let bank = CurveBank::new();
        let mut shifted = CurveShape::linear();
        shifted.move_knot(0, 0.0, 0.5);
        bank.store(Slot::NoteOff, &shifted);
        bank.set_hi_res(true);
        let mut p = Prefix::new();
        let out = run(&bank, &mut p, &[&[0xB0, 88, 99], &[0x90, 61, 0]]);
        assert_eq!(out, vec![vec![0x90, 61, 0]]);
        assert_eq!(p.take(0), None);
    }
}
