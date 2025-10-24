//! Модуль для работы с кривыми Безье

pub mod bezier;
pub mod control_point;
pub mod interpolation;

pub use bezier::BezierCurve;
pub use control_point::ControlPoint;