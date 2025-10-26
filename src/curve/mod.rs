/// Модуль для работы с кривыми Безье и обработкой MIDI velocity
pub mod bezier;
pub mod interpolation;
pub mod control_point;
pub mod dual_curve;

pub use bezier::BezierCurve;
pub use control_point::ControlPoint;
pub use interpolation::{cubic_bezier};
pub use dual_curve::DualCurve;