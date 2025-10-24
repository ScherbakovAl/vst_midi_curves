use nih_plug::prelude::*;
use vst_midi_curves::MidiCurvesPlugin;

fn main() {
    nih_export_standalone::<MidiCurvesPlugin>();
}