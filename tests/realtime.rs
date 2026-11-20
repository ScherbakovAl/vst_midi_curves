//! Путь MIDI под требования реального времени (§2.1 `.work/AUDIT_PLAN.md`):
//! ни одной аллокации на событие, корректный выход при одновременной правке
//! кривой из другого потока, время на событие.
//!
//! Стресс-тест и бенчмарк — в release:
//!   cargo test --release --test realtime -- --include-ignored --nocapture

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use nih_plug::prelude::NoteEvent;
use vst_midi_curves::curve::{CurveBank, CurveShape, Slot};
use vst_midi_curves::process_event;
use vst_midi_curves::velocity::{process_midi, Prefix};

/// Глобальный аллокатор со счётчиком на поток: тесты идут параллельно, считаем
/// только свой поток.
struct Counting;

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

fn bump() {
    let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        bump();
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        bump();
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        bump();
        unsafe { System.realloc(p, l, n) }
    }
}

#[global_allocator]
static A: Counting = Counting;

fn allocs() -> usize {
    ALLOCS.with(|c| c.get())
}

/// Поток сообщений клавиатуры: CC88 перед каждой нотой, On и Off, 4 канала.
fn message(i: u32) -> [u8; 3] {
    let ch = (i / 4 % 4) as u8;
    let note = 21 + (i / 16 % 88) as u8;
    let v = (i.wrapping_mul(2654435761) >> 25) as u8; // 0…127
    match i % 4 {
        0 | 2 => [0xB0 | ch, 88, v],
        1 => [0x90 | ch, note, v.max(1)],
        _ => [0x80 | ch, note, v],
    }
}

fn s_curve() -> CurveShape {
    let mut s = CurveShape::linear();
    s.set_handle(0, true, [0.6, 0.0], false);
    s.set_handle(1, false, [0.4, 1.0], false);
    s
}

fn steep() -> CurveShape {
    let mut s = CurveShape::linear();
    s.knots[0].hout = [0.02, 0.6];
    s.knots[1].hin = [-0.9, -0.05];
    s
}

#[test]
fn no_allocations_on_the_midi_path() {
    let bank = CurveBank::new();
    bank.store(Slot::NoteOn, &s_curve());
    let mut prefix = Prefix::new();
    let mut sink = [0u8; 3];
    let mut emitted = 0usize;
    for hi in [false, true] {
        bank.set_hi_res(hi);
        let before = allocs();
        for i in 0..10_000 {
            let m = message(i);
            process_midi(&bank, &mut prefix, &m, |o| {
                sink[..o.len().min(3)].copy_from_slice(&o[..o.len().min(3)]);
                emitted += 1;
            });
        }
        assert_eq!(allocs() - before, 0, "аллокации на пути MIDI (hi-res = {hi})");
    }
    assert!(emitted > 0);
}

/// То же для события аудиопотока плагина (`process()` зовёт только это).
#[test]
fn no_allocations_in_plugin_process_event() {
    let bank = CurveBank::new();
    bank.store(Slot::NoteOff, &steep());
    let mut prefix = Prefix::new();
    let mut last = None;
    for hi in [false, true] {
        bank.set_hi_res(hi);
        let before = allocs();
        for i in 0..10_000u32 {
            let [st, d1, d2] = message(i);
            let (timing, channel) = (i % 512, st & 0x0F);
            let ev: NoteEvent<()> = match st & 0xF0 {
                0xB0 => NoteEvent::MidiCC { timing, channel, cc: d1, value: d2 as f32 / 127.0 },
                0x90 => NoteEvent::NoteOn { timing, voice_id: None, channel, note: d1, velocity: d2 as f32 / 127.0 },
                _ => NoteEvent::NoteOff { timing, voice_id: None, channel, note: d1, velocity: d2 as f32 / 127.0 },
            };
            process_event(&bank, &mut prefix, ev, |o| last = Some(o));
        }
        assert_eq!(allocs() - before, 0, "аллокации в process_event (hi-res = {hi})");
    }
    assert!(last.is_some());
}

/// Поток-писатель непрерывно перезаливает таблицы двумя разными формами и
/// дёргает формат выхода; поток-читатель гонит события. Выход всегда в
/// допустимом диапазоне, Note On никогда не с velocity 0 (HH = 0), без паник.
#[test]
fn writer_reader_stress() {
    let events: u32 = if cfg!(debug_assertions) { 1_000_000 } else { 10_000_000 };
    let bank = Arc::new(CurveBank::new());
    let stop = Arc::new(AtomicBool::new(false));
    let writer = {
        let (bank, stop) = (bank.clone(), stop.clone());
        std::thread::spawn(move || {
            let shapes = [s_curve(), steep()];
            let mut n = 0usize;
            while !stop.load(Ordering::Relaxed) {
                for slot in Slot::ALL {
                    bank.store(slot, &shapes[n % 2]);
                }
                bank.set_hi_res(n % 3 != 0);
                n += 1;
            }
            n
        })
    };
    let mut prefix = Prefix::new();
    let mut last_cc88 = [false; 16];
    for i in 0..events {
        let m = message(i);
        process_midi(&bank, &mut prefix, &m, |o| {
            assert!(o.len() == 3 && o[1] < 128 && o[2] < 128, "битое сообщение {o:?}");
            let ch = (o[0] & 0x0F) as usize;
            match o[0] & 0xF0 {
                0xB0 => {
                    assert_eq!(o[1], 88);
                    last_cc88[ch] = true;
                }
                0x90 => {
                    assert!(o[2] >= 1, "Note On с velocity 0: {o:?}");
                    last_cc88[ch] = false;
                }
                0x80 => last_cc88[ch] = false,
                _ => panic!("неожиданное сообщение {o:?}"),
            }
        });
    }
    stop.store(true, Ordering::Relaxed);
    let rewrites = writer.join().unwrap();
    assert!(rewrites > 0);
    assert!(last_cc88.iter().all(|&p| !p), "CC88 без ноты следом");
}

/// Время на событие (release): цель < 1 мкс на сообщение.
#[test]
#[ignore]
fn bench_ns_per_event() {
    let bank = CurveBank::new();
    bank.store(Slot::NoteOn, &s_curve());
    bank.store(Slot::NoteOff, &steep());
    let mut prefix = Prefix::new();
    let n = 4_000_000u32;
    let msgs: Vec<[u8; 3]> = (0..n).map(message).collect();
    for hi in [false, true] {
        bank.set_hi_res(hi);
        let mut acc = 0u32;
        let t = Instant::now();
        for m in &msgs {
            process_midi(&bank, &mut prefix, std::hint::black_box(m), |o| acc = acc.wrapping_add(o[2] as u32));
        }
        let ns = t.elapsed().as_nanos() as f64 / n as f64;
        println!("hi-res = {hi}: {ns:.1} нс на сообщение (контроль {acc})");
        assert!(ns < 1000.0);
    }
}
