//! Реализация кривой Безье для обработки MIDI velocity

use kurbo::Point;
use super::ControlPoint;

#[derive(Debug, Clone)]
pub struct BezierCurve {
    control_points: Vec<ControlPoint>,
    cached_lut: Vec<f32>, // Look-up table для оптимизации (128 значений для MIDI velocity)
}

impl BezierCurve {
    pub fn new() -> Self {
        // Инициализация с линейной кривой (y = x)
        let control_points = vec![
            ControlPoint::new(Point::new(0.0, 0.0)),
            ControlPoint::new(Point::new(127.0, 127.0)),
        ];

        let mut curve = Self {
            control_points,
            cached_lut: Vec::new(),
        };
        curve.rebuild_lut();
        curve
    }

    /// Создает линейную кривую (y = x)
    pub fn linear() -> Self {
        Self::new()
    }

    /// Вычисление значения кривой в точке x (0.0 - 1.0)
    pub fn evaluate(&self, x: f32) -> f32 {
        let x_normalized = (x * 127.0).clamp(0.0, 127.0);
        
        // Использование LUT для быстрого доступа
        if self.cached_lut.is_empty() {
            return x; // Fallback к линейной кривой
        }

        let index = x_normalized as usize;
        let fraction = x_normalized - index as f32;

        if index >= self.cached_lut.len() - 1 {
            return self.cached_lut[self.cached_lut.len() - 1];
        }

        // Линейная интерполяция между точками LUT
        self.cached_lut[index] * (1.0 - fraction) + self.cached_lut[index + 1] * fraction
    }

    /// Добавление новой контрольной точки
    pub fn add_control_point(&mut self, position: Point) {
        let new_point = ControlPoint::new(position);
        
        // Находим правильную позицию для вставки (по x координате)
        let insert_index = self.control_points
            .iter()
            .position(|p| p.position.x > position.x)
            .unwrap_or(self.control_points.len());
            
        self.control_points.insert(insert_index, new_point);
        self.rebuild_lut();
    }

    /// Удаление контрольной точки (кроме первой и последней)
    pub fn remove_control_point(&mut self, index: usize) {
        if index > 0 && index < self.control_points.len() - 1 {
            self.control_points.remove(index);
            self.rebuild_lut();
        }
    }

    /// Обновление позиции контрольной точки
    pub fn update_control_point(&mut self, index: usize, position: Point) {
        if let Some(point) = self.control_points.get_mut(index) {
            point.update_position(position);
            self.rebuild_lut();
        }
    }

    /// Получение списка контрольных точек
    pub fn control_points(&self) -> &[ControlPoint] {
        &self.control_points
    }

    /// Перестроение таблицы значений для быстрого доступа
    fn rebuild_lut(&mut self) {
        self.cached_lut.clear();
        
        // Создаем LUT с 128 точками (0-127 для MIDI velocity)
        for i in 0..128 {
            let x = i as f32;
            let y = self.evaluate_bezier_direct(x);
            self.cached_lut.push(y / 127.0); // Нормализуем к диапазону 0.0-1.0
        }
    }

    /// Вычисление значения кривой Безье для заданного x (без использования LUT)
    fn evaluate_bezier_direct(&self, x: f32) -> f32 {
        if self.control_points.len() < 2 {
            return x;
        }

        // Для простоты используем линейную интерполяцию между контрольными точками
        // В будущем можно реализовать кубические кривые Безье
        for i in 0..self.control_points.len() - 1 {
            let p1 = &self.control_points[i];
            let p2 = &self.control_points[i + 1];
            
            let p1_x = p1.position.x as f32;
            let p2_x = p2.position.x as f32;
            let p1_y = p1.position.y as f32;
            let p2_y = p2.position.y as f32;
            
            if x >= p1_x && x <= p2_x {
                let t = (x - p1_x) / (p2_x - p1_x);
                return p1_y + t * (p2_y - p1_y);
            }
        }
        
        // Если x вне диапазона, возвращаем ближайшее значение
        if x < self.control_points[0].position.x as f32 {
            self.control_points[0].position.y as f32
        } else {
            self.control_points[self.control_points.len() - 1].position.y as f32
        }
    }
}

impl Default for BezierCurve {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_curve() {
        let curve = BezierCurve::new();
        
        // Проверяем линейную кривую (y = x)
        assert!((curve.evaluate(0.0) - 0.0).abs() < 0.001);
        assert!((curve.evaluate(0.5) - 0.5).abs() < 0.001);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 0.001);
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
    fn test_lut_rebuilding() {
        let curve = BezierCurve::new();
        
        // Проверяем, что LUT строится корректно
        assert_eq!(curve.cached_lut.len(), 128);
        
        // Проверяем несколько значений с учетом ошибок округления
        assert!((curve.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((curve.evaluate(0.5) - 0.5).abs() < 0.01);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 0.01);
    }
}