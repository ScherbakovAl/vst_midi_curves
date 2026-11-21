//! MIDI Curves — VST3/CLAP-плагин: velocity нот через редактируемые кривые
//! (Note On и Note Off отдельно), вход 7 бит или 14 бит с префиксом CC88,
//! выход 7 или 14 бит.
//!
//! Copyright (c) 2025 Scherbakov Alexey <scherbakov.al@gmail.com>
//! Licensed under the MIT License
//!
//! Потоки: `process()` — аудиопоток хоста (только атомики банка и префикс,
//! которым владеет сам); GUI (baseview) правит кривые и перезаливает таблицы;
//! главный поток хоста сохраняет/загружает `#[persist]`-состояние.

pub mod curve;
pub mod editor;
pub mod library;
pub mod settings;
pub mod velocity;

/// egui той же версии, что у nih_plug_egui (её же использует standalone).
pub use nih_plug_egui::egui;

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard};
use std::time::{Duration, Instant};

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, EguiState};

use crate::curve::{CurveBank, CurveSet, Slot};
use crate::editor::CurvesUi;
use crate::library::Library;
use crate::velocity::{decode, map_note, Prefix, CC_HIRES_VELOCITY};

/// Параметров-ручек нет: всё состояние — `#[persist]`-поля, сохраняются в
/// проекте DAW.
#[derive(Params)]
pub struct MidiCurvesParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,
    /// Карты нот обеих кривых и формат выхода. Ключ тот же, что у формата
    /// с одной формой на кривую: старые проекты читаются (`NoteMap`).
    #[persist = "curves"]
    curves: Arc<RwLock<CurveSet>>,
    /// Открытая вкладка редактора (0 — Note On, 1 — Note Off).
    #[persist = "tab"]
    tab: Arc<AtomicU8>,
}

impl Default for MidiCurvesParams {
    fn default() -> Self {
        Self {
            // Старые проекты хранят свой размер окна (поле "editor-state").
            editor_state: EguiState::from_size(1000, 800),
            curves: Arc::new(RwLock::new(CurveSet::default())),
            tab: Arc::new(AtomicU8::new(0)),
        }
    }
}

pub struct MidiCurvesPlugin {
    params: Arc<MidiCurvesParams>,
    /// Таблицы кривых для `process()`; GUI перезаливает их при правке.
    bank: Arc<CurveBank>,
    /// Ожидающие CC88 по каналам — владеет только аудиопоток.
    prefix: Prefix,
}

impl Default for MidiCurvesPlugin {
    fn default() -> Self {
        // Конструктор вызывается и при сканировании плагинов: без файлового
        // ввода-вывода. Библиотеку форм GUI читает лениво.
        Self { params: Arc::new(MidiCurvesParams::default()), bank: Arc::new(CurveBank::new()), prefix: Prefix::new() }
    }
}

/// Чтение кривых без паники на «отравленной» блокировке (`panic = "abort"`:
/// паника уронила бы хост).
fn read(l: &RwLock<CurveSet>) -> RwLockReadGuard<'_, CurveSet> {
    l.read().unwrap_or_else(|e| e.into_inner())
}

fn slot_from(tab: u8) -> Slot {
    if tab == 1 {
        Slot::NoteOff
    } else {
        Slot::NoteOn
    }
}

/// Значение NoteEvent (0…1) → 7 бит.
#[inline]
fn to7(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 127.0).round() as u8
}

/// Одно событие аудиопотока: CC88 → префикс канала (дальше не идёт), Note
/// On/Off → через кривую (в hi-res — CC88 перед нотой в том же сэмпле),
/// остальное — как есть. Без аллокаций и блокировок. Публична для тестов.
#[inline]
pub fn process_event(
    bank: &CurveBank,
    prefix: &mut Prefix,
    event: NoteEvent<()>,
    mut send: impl FnMut(NoteEvent<()>),
) {
    // Нота без префикса берёт velocity как есть (у 7-битных хостов это HH/127;
    // у хоста с дробной velocity — без потери точности).
    let input = |prefix: &mut Prefix, channel: u8, velocity: f32| match prefix.take(channel) {
        Some(ll) => decode(to7(velocity), Some(ll)),
        None => velocity,
    };
    match event {
        NoteEvent::MidiCC { channel, cc: CC_HIRES_VELOCITY, value, .. } => prefix.set(channel, to7(value)),
        NoteEvent::NoteOn { timing, voice_id, channel, note, velocity } if velocity <= 0.0 => {
            // Note On с velocity 0 — это Note Off без данных: префикс расходуется,
            // событие уходит как есть.
            prefix.take(channel);
            send(NoteEvent::NoteOn { timing, voice_id, channel, note, velocity });
        }
        NoteEvent::NoteOn { timing, voice_id, channel, note, velocity } => {
            let out = map_note(bank, Slot::NoteOn, note, input(prefix, channel, velocity));
            if let Some(ll) = out.ll {
                send(NoteEvent::MidiCC { timing, channel, cc: CC_HIRES_VELOCITY, value: ll as f32 / 127.0 });
            }
            send(NoteEvent::NoteOn { timing, voice_id, channel, note, velocity: out.vel as f32 / 127.0 });
        }
        NoteEvent::NoteOff { timing, voice_id, channel, note, velocity } => {
            let out = map_note(bank, Slot::NoteOff, note, input(prefix, channel, velocity));
            if let Some(ll) = out.ll {
                send(NoteEvent::MidiCC { timing, channel, cc: CC_HIRES_VELOCITY, value: ll as f32 / 127.0 });
            }
            send(NoteEvent::NoteOff { timing, voice_id, channel, note, velocity: out.vel as f32 / 127.0 });
        }
        other => send(other),
    }
}

/// Состояние окна плагина (живёт в `create_egui_editor`, пока жив плагин).
struct GuiState {
    set: CurveSet,
    ui: CurvesUi,
    opened: Instant,
    last_kick: Instant,
    seen_notes: u64,
}

/// Перерисовка: egui-baseview рисует кадр, только когда egui просит
/// (`request_repaint_after` он не соблюдает — каждый кадр сдвигает срок).
/// Первую секунду после открытия — каждый кадр (кадр, отрисованный до показа
/// окна, терялся: чёрный экран в Reaper), потом — маркер ноты не чаще 30 Гц и
/// контрольная перерисовка 4 Гц. Закрытое окно не стоит ничего.
const FRESH: Duration = Duration::from_secs(1);
const MARKER_PERIOD: Duration = Duration::from_millis(33);
const KEEPALIVE: Duration = Duration::from_millis(250);

impl Plugin for MidiCurvesPlugin {
    const NAME: &'static str = "MIDI Curves";
    const VENDOR: &'static str = "Scherbakov Alexey";
    const URL: &'static str = "https://github.com/ScherbakovAl/vst_midi_curves";
    const EMAIL: &'static str = "scherbakov.al@gmail.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // MIDI-only plugin with minimal stereo layout for Reaper compatibility
    // Reaper requires at least stereo input/output, even for MIDI-only plugins
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    // MidiCCs required for hi-res MIDI support (CC#88 messages)
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::MidiCCs;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type BackgroundTask = ();
    type SysExMessage = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let bank = self.bank.clone();
        let set = read(&params.curves).clone();
        let tab = slot_from(params.tab.load(Ordering::Relaxed));
        let now = Instant::now();
        let state = GuiState {
            ui: CurvesUi::new(&set, tab, Library::new(library::default_dir())),
            set,
            opened: now,
            last_kick: now,
            seen_notes: 0,
        };
        create_egui_editor(
            params.editor_state.clone(),
            state,
            |ctx, st| {
                // Каждое открытие окна: библиотеку перечитать (её мог менять
                // standalone), первая секунда — частая перерисовка.
                crate::editor::setup_fonts(ctx);
                st.opened = Instant::now();
                st.ui.rescan_library();
            },
            move |ctx, _setter, st| {
                // Состояние загрузил хост (проект, пресет): принять, история заново.
                {
                    let persisted = read(&params.curves);
                    if *persisted != st.set {
                        st.set = persisted.clone();
                        st.set.apply(&bank);
                        st.ui.reset(&st.set);
                    }
                }
                let now = Instant::now();
                let notes = bank.notes();
                let since = now.duration_since(st.last_kick);
                if now.duration_since(st.opened) < FRESH
                    || (notes != st.seen_notes && since >= MARKER_PERIOD)
                    || since >= KEEPALIVE
                {
                    st.seen_notes = notes;
                    st.last_kick = now;
                    ctx.request_repaint();
                }

                let out = st.ui.show(ctx, &mut st.set, &bank, |_| {});
                out.store(&st.set, &bank);
                if out.changed() || out.hi_res {
                    *params.curves.write().unwrap_or_else(|e| e.into_inner()) = st.set.clone();
                }
                if out.tab {
                    params.tab.store(st.ui.tab.idx() as u8, Ordering::Relaxed);
                }
            },
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        _buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        // Вызывается и после загрузки состояния хостом: починить формы (их
        // увидит и GUI) и залить таблицы.
        let mut set = self.params.curves.write().unwrap_or_else(|e| e.into_inner());
        set.normalize();
        set.apply(&self.bank);
        true
    }

    fn reset(&mut self) {
        self.prefix = Prefix::new();
    }

    fn process(
        &mut self,
        _buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        while let Some(event) = context.next_event() {
            process_event(&self.bank, &mut self.prefix, event, |e| context.send_event(e));
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for MidiCurvesPlugin {
    // НЕ менять: по нему хост находит плагин в сохранённых проектах.
    const VST3_CLASS_ID: [u8; 16] = *b"MidiCurvesVST3!!";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

impl ClapPlugin for MidiCurvesPlugin {
    const CLAP_ID: &'static str = "io.github.scherbakoval.midi-curves";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("MIDI velocity curves (Note On / Note Off, 14-bit CC88)");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = Some("https://github.com/ScherbakovAl/vst_midi_curves/issues");
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::NoteEffect, ClapFeature::Utility];
}

// Единственное предупреждение сборки — «trailing semicolon in macro» из
// макроса vst3_com внутри nih_export_vst3! (nih-plug 28b149e); allow его не
// гасит (future-incompat lint из чужого макроса).
nih_export_vst3!(MidiCurvesPlugin);
nih_export_clap!(MidiCurvesPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::CurveShape;

    fn run(bank: &CurveBank, prefix: &mut Prefix, events: &[NoteEvent<()>]) -> Vec<NoteEvent<()>> {
        let mut out = Vec::new();
        for &e in events {
            process_event(bank, prefix, e, |o| out.push(o));
        }
        out
    }

    fn cc(timing: u32, channel: u8, cc: u8, v: u8) -> NoteEvent<()> {
        NoteEvent::MidiCC { timing, channel, cc, value: v as f32 / 127.0 }
    }

    fn on(timing: u32, channel: u8, note: u8, v: u8) -> NoteEvent<()> {
        NoteEvent::NoteOn { timing, voice_id: None, channel, note, velocity: v as f32 / 127.0 }
    }

    fn off(timing: u32, channel: u8, note: u8, v: u8) -> NoteEvent<()> {
        NoteEvent::NoteOff { timing, voice_id: None, channel, note, velocity: v as f32 / 127.0 }
    }

    #[test]
    fn diagonal_hi_res_passes_14_bit_note_on_and_off_unchanged() {
        // Тот же поток, что шлёт клавиатура: CC88 перед каждым ON и OFF. LL
        // Note Off больше не портится (раньше LL и HH шли через кривую врозь).
        let bank = CurveBank::new();
        bank.set_hi_res(true);
        let mut p = Prefix::new();
        let input = [cc(5, 0, 88, 37), on(5, 0, 60, 64), cc(90, 0, 88, 101), off(90, 0, 60, 12)];
        let out = run(&bank, &mut p, &input);
        assert_eq!(out, input.to_vec());
    }

    #[test]
    fn seven_bit_output_drops_cc88_and_keeps_timing() {
        let bank = CurveBank::new();
        let mut s = CurveShape::linear();
        s.move_knot(1, 1.0, 0.5);
        bank.store(Slot::NoteOn, &s);
        let mut p = Prefix::new();
        // Вход (127·128 + 0)/16383 ≈ 0.992, кривая y = x/2 → 63 (7 бит).
        let out = run(&bank, &mut p, &[cc(7, 2, 88, 0), on(7, 2, 61, 127), cc(9, 2, 64, 127)]);
        assert_eq!(out, vec![on(7, 2, 61, 63), cc(9, 2, 64, 127)]);
    }

    #[test]
    fn chord_in_one_sample_and_channels() {
        // CLAP: события по порядку — у каждой ноты свой префикс.
        let bank = CurveBank::new();
        bank.set_hi_res(true);
        let mut p = Prefix::new();
        let input =
            [cc(0, 0, 88, 10), on(0, 0, 60, 70), cc(0, 0, 88, 20), on(0, 0, 64, 71), cc(0, 1, 88, 30), on(0, 1, 67, 72)];
        assert_eq!(run(&bank, &mut p, &input), input.to_vec());
    }

    #[test]
    fn note_on_zero_passes_and_note_on_never_zero() {
        let bank = CurveBank::new();
        let mut zero = CurveShape::linear();
        zero.move_knot(1, 1.0, 0.0);
        bank.store(Slot::NoteOn, &zero);
        let mut p = Prefix::new();
        let z = NoteEvent::NoteOn { timing: 3, voice_id: None, channel: 0, note: 60, velocity: 0.0 };
        let out = run(&bank, &mut p, &[cc(3, 0, 88, 5), z, on(4, 0, 60, 90)]);
        assert_eq!(out, vec![z, on(4, 0, 60, 1)]);
    }
}
