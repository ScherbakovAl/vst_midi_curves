//! Обработчик velocity с использованием кривых Безье

use crate::curve::BezierCurve;

#[derive(Debug, Clone)]
pub struct VelocityCurveProcessor {
    curve: BezierCurve,
}

impl VelocityCurveProcessor {
    pub fn new() -> Self {
        Self {
            curve: BezierCurve::new(),
        }
    }

    pub fn process_velocity(&self, input_velocity: u8) -> u8 {
        let normalized_input = input_velocity as f32 / 127.0;
        let normalized_output = self.curve.evaluate(normalized_input);
        (normalized_output * 127.0).clamp(0.0, 127.0) as u8
    }

    pub fn set_curve(&mut self, curve: BezierCurve) {
        self.curve = curve;
    }

    pub fn curve(&self) -> &BezierCurve {
        &self.curve
    }

    pub fn curve_mut(&mut self) -> &mut BezierCurve {
        &mut self.curve
    }
}

impl Default for VelocityCurveProcessor {
    fn default() -> Self {
        Self::new()
    }
}