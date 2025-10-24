use nih_plug::prelude::*;
use std::sync::Arc;

struct MidiCurvesPlugin {
    params: Arc<MidiCurvesParams>,
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
        }
    }
}

impl Plugin for MidiCurvesPlugin {
    const NAME: &'static str = "MIDI Curves";
    const VENDOR: &'static str = "Your Name";
    const URL: &'static str = "https://yoursite.com";
    const EMAIL: &'static str = "your@email.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[];

    type BackgroundTask = ();
    type SysExMessage = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn process(
        &mut self,
        _buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { timing, voice_id, channel, note, velocity } => {
                    // Пока просто пропускаем MIDI события без обработки
                    context.send_event(NoteEvent::NoteOn {
                        timing,
                        voice_id,
                        channel,
                        note,
                        velocity,
                    });
                }
                _ => context.send_event(event),
            }
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for MidiCurvesPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"MidiCurvesPlugin";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx];
}

nih_plug::nih_export_vst3!(MidiCurvesPlugin);