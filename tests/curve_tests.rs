//! Тесты для кривых Безье

use vst_midi_curves::curve::BezierCurve;
use kurbo::Point;

#[test]
fn test_linear_curve() {
    let curve = BezierCurve::new();
    
    // Проверяем линейную кривую (y = x)
    assert_eq!(curve.evaluate(0.0), 0.0);
    assert_eq!(curve.evaluate(0.5), 0.5);
    assert_eq!(curve.evaluate(1.0), 1.0);
}

#[test]
fn test_control_points() {
    let curve = BezierCurve::new();
    let points = curve.control_points();
    
    // Проверяем начальные контрольные точки
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].position.x, 0.0);
    assert_eq!(points[0].position.y, 0.0);
    assert_eq!(points[1].position.x, 127.0);
    assert_eq!(points[1].position.y, 127.0);
}

#[test]
fn test_velocity_processing() {
    use vst_midi_curves::processor::VelocityCurveProcessor;
    
    let processor = VelocityCurveProcessor::new();
    
    // Проверяем линейное преобразование velocity
    assert_eq!(processor.process_velocity(0), 0);
    assert_eq!(processor.process_velocity(64), 64);
    assert_eq!(processor.process_velocity(127), 127);
}