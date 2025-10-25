use nih_plug::prelude::*;
use std::sync::Arc;

// Подключаем модули для работы с кривыми
pub mod curve;
pub mod presets;

use curve::BezierCurve;

// MIDI плагин с обработкой velocity кривых
struct MidiCurvesPlugin {
    params: Arc<MidiCurvesParams>,
    curve_processor: BezierCurve,
}

#[derive(Params)]
struct MidiCurvesParams {}

impl Default for MidiCurvesParams {
    fn default() -> Self {
        Self {}
    }
}

impl Default for MidiCurvesPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(MidiCurvesParams::default()),
            curve_processor: BezierCurve::new(),
        }
    }
}

impl Plugin for MidiCurvesPlugin {
    const NAME: &'static str = "MIDI Curves";
    const VENDOR: &'static str = "Your Name";
    const URL: &'static str = "https://yoursite.com";
    const EMAIL: &'static str = "your@email.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    type BackgroundTask = ();
    type SysExMessage = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn process(
        &mut self,
        _buffer: &mut Buffer,
        _auxiliary_buffers: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Обрабатываем MIDI события с применением кривой
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { timing, voice_id, channel, note, velocity } => {
                    // Применяем кривую к velocity
                    let input_velocity = (velocity * 127.0) as f32;
                    let processed_velocity = self.curve_processor.evaluate(input_velocity);
                    let normalized_velocity = (processed_velocity / 127.0).clamp(0.0, 1.0);
                    
                    context.send_event(NoteEvent::NoteOn {
                        timing,
                        voice_id,
                        channel,
                        note,
                        velocity: normalized_velocity,
                    });
                }
                NoteEvent::NoteOff { timing, voice_id, channel, note, velocity } => {
                    // NoteOff пропускаем без изменений
                    context.send_event(NoteEvent::NoteOff {
                        timing,
                        voice_id,
                        channel,
                        note,
                        velocity,
                    });
                }
                _ => {
                    // Остальные события пропускаем как есть
                    context.send_event(event);
                }
            }
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for MidiCurvesPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"MidiCurvesPlugin";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[];
}

nih_plug::nih_export_vst3!(MidiCurvesPlugin);
