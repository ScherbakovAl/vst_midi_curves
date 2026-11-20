//! Модуль для работы с кривыми Безье и обработкой MIDI velocity
//!
//! Copyright (c) 2025 Scherbakov Alexey <scherbakov.al@gmail.com>
//! Licensed under the MIT License

pub mod bezier;
pub mod interpolation;
pub mod control_point;
pub mod dual_curve;
pub mod shape;
pub mod bank;
pub mod legacy;

pub use bezier::BezierCurve;
pub use control_point::ControlPoint;
pub use interpolation::{cubic_bezier};
pub use dual_curve::DualCurve;
pub use shape::{CurveShape, Knot};
pub use bank::{CurveBank, Slot, LUT_N};