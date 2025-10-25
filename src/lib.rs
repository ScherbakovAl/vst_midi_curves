use nih_plug::prelude::*;
use std::sync::Arc;

pub mod curve;
pub mod editor;
pub mod gui;
pub mod processor;
use processor::VelocityCurveProcessor;

pub struct MidiCurvesPlugin {
    params: Arc<MidiCurvesParams>,
    curve_processor: Arc<std::sync::Mutex<VelocityCurveProcessor>>,
}

#[derive(Params)]
struct MidiCurvesParams {
    #[id = "curve_enabled"]
    pub curve_enabled: BoolParam,
}

impl Default for MidiCurvesParams {
    fn default() -> Self {
        Self {
            curve_enabled: BoolParam::new("Curve Enabled", true),
        }
    }
}

impl Default for MidiCurvesPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(MidiCurvesParams::default()),
            curve_processor: Arc::new(std::sync::Mutex::new(VelocityCurveProcessor::new())),
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

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create_editor(self.curve_processor.clone())
    }

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
                    if self.params.curve_enabled.value() {
                        let processed_velocity = self.curve_processor.lock().unwrap()
                            .process_velocity((velocity * 127.0) as u8);
                        
                        context.send_event(NoteEvent::NoteOn {
                            timing,
                            voice_id,
                            channel,
                            note,
                            velocity: processed_velocity as f32 / 127.0,
                        });
                    } else {
                        // Если кривая отключена, просто пропускаем событие
                        context.send_event(event);
                    }
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