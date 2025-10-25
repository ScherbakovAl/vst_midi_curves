/// Основная структура для работы с кривыми Безье
use crate::curve::{ControlPoint, cubic_bezier};

#[derive(Debug, Clone)]
pub struct BezierCurve {
    /// Контрольные точки кривой
    pub control_points: Vec<ControlPoint>,
    /// Кэшированная таблица значений для быстрого доступа (LUT)
    cached_lut: Vec<(f32, f32)>,
    /// Флаг, указывающий нужно ли обновить кэш
    dirty: bool,
}

impl BezierCurve {
    /// Создает новую линейную кривую (y = x) по умолчанию
    pub fn new() -> Self {
        let default_curve = vec![
            ControlPoint::new((0.0, 0.0)),
            ControlPoint::new((127.0, 127.0)),
        ];
        
        Self {
            control_points: default_curve,
            cached_lut: Vec::new(),
            dirty: true,
        }
    }

    /// Создает кривую из заданных контрольных точек
    pub fn from_points(points: Vec<ControlPoint>) -> Self {
        Self {
            control_points: points,
            cached_lut: Vec::new(),
            dirty: true,
        }
    }

    /// Вычисляет значение кривой в точке x
    /// 
    /// # Параметры:
    /// - x: входное значение от 0.0 до 127.0
    /// 
    /// # Возвращает:
    /// Выходное значение от 0.0 до 127.0
    pub fn evaluate(&mut self, x: f32) -> f32 {
        self.ensure_cache_updated();
        
        // Проверяем кэш для быстрого поиска
        for &(cached_x, cached_y) in &self.cached_lut {
            if (cached_x - x).abs() < 0.5 {
                return cached_y;
            }
        }
        
        // Если не нашли в кэше, вычисляем напрямую
        self.evaluate_directly(x)
    }

    /// Прямое вычисление значения кривой (без кэша)
    fn evaluate_directly(&self, x: f32) -> f32 {
        if self.control_points.len() < 2 {
            return x;
        }
        
        let normalized_x = x / 127.0;
        let mut y_value = 0.0;
        
        if self.control_points.len() == 2 {
            // Линейная интерполяция
            let (x1, y1) = self.control_points[0].position;
            let (x2, y2) = self.control_points[1].position;
            let ratio = if x2 != x1 { (x - x1) / (x2 - x1) } else { 0.0 };
            y_value = y1 + (y2 - y1) * ratio;
        } else {
            // Используем первые 4 точки для кубической кривой
            let p0 = self.control_points[0].position;
            let p1 = self.control_points.get(1).unwrap_or(&self.control_points[0]).position;
            let p2 = self.control_points.get(2).unwrap_or(&self.control_points[1]).position;
            let p3 = self.control_points.get(3).unwrap_or(&self.control_points[2]).position;
            
            // Находим t для заданного x
            let t = self.find_t_for_x(x, &p0, &p1, &p2, &p3);
            
            // Вычисляем y
            y_value = cubic_bezier(t, p0.1, p1.1, p2.1, p3.1);
        }
        
        y_value.clamp(0.0, 127.0)
    }

    /// Находит параметр t для заданного x методом Ньютона-Рафсона
    fn find_t_for_x(&self, target_x: f32, p0: &(f32, f32), p1: &(f32, f32), p2: &(f32, f32), p3: &(f32, f32)) -> f32 {
        let mut t = 0.5; // Начальное приближение
        
        for _ in 0..5 { // 5 итераций для точности
            let x_t = cubic_bezier(t, p0.0, p1.0, p2.0, p3.0);
            let dx_dt = cubic_bezier_derivative(t, p0.0, p1.0, p2.0, p3.0);
            
            if dx_dt.abs() < 0.001 {
                break; // Избегаем деления на ноль
            }
            
            let error = x_t - target_x;
            t -= error / dx_dt;
            
            // Ограничиваем t диапазоном [0, 1]
            t = t.clamp(0.0, 1.0);
        }
        
        t
    }

    /// Добавляет новую контрольную точку
    pub fn add_control_point(&mut self, position: (f32, f32)) {
        let mut new_point = ControlPoint::new(position);
        new_point.clamp_to_midi_range();
        
        // Вставляем точку в отсортированном порядке по x
        let insert_index = self.control_points
            .iter()
            .position(|p| p.position.0 > position.0)
            .unwrap_or(self.control_points.len());
        
        self.control_points.insert(insert_index, new_point);
        self.dirty = true;
    }

    /// Удаляет контрольную точку по индексу (кроме первой и последней)
    pub fn remove_control_point(&mut self, index: usize) -> bool {
        if index > 0 && index < self.control_points.len() - 1 {
            self.control_points.remove(index);
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Обновляет позицию контрольной точки
    pub fn update_control_point(&mut self, index: usize, position: (f32, f32)) -> bool {
        if let Some(point) = self.control_points.get_mut(index) {
            point.set_position(position);
            point.clamp_to_midi_range();
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Сбрасывает кривую к линейной (y = x)
    pub fn reset_to_linear(&mut self) {
        self.control_points = vec![
            ControlPoint::new((0.0, 0.0)),
            ControlPoint::new((127.0, 127.0)),
        ];
        self.dirty = true;
    }

    /// Обеспечивает обновление кэша
    fn ensure_cache_updated(&mut self) {
        if self.dirty {
            self.rebuild_lut();
            self.dirty = false;
        }
    }

    /// Перестраивает таблицу значений (LUT) для быстрого доступа
    fn rebuild_lut(&mut self) {
        self.cached_lut.clear();
        
        // Создаем таблицу из 128 значений (0-127)
        for i in 0..=127 {
            let x = i as f32;
            let y = self.evaluate_directly(x);
            self.cached_lut.push((x, y));
        }
    }

    /// Возвращает ссылку на кэшированные значения
    pub fn get_cached_values(&self) -> &[(f32, f32)] {
        &self.cached_lut
    }
}

impl Default for BezierCurve {
    fn default() -> Self {
        Self::new()
    }
}

/// Производная от кубической кривой Безье (для метода Ньютона-Рафсона)
fn cubic_bezier_derivative(t: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    let mt = 1.0 - t;
    3.0 * mt * mt * (p1 - p0) + 6.0 * mt * t * (p2 - p1) + 3.0 * t * t * (p3 - p2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_curve() {
        let mut curve = BezierCurve::new();
        assert_eq!(curve.evaluate(0.0), 0.0);
        assert_eq!(curve.evaluate(64.0), 64.0);
        assert_eq!(curve.evaluate(127.0), 127.0);
    }

    #[test]
    fn test_curve_with_points() {
        let points = vec![
            ControlPoint::new((0.0, 0.0)),
            ControlPoint::new((64.0, 32.0)),
            ControlPoint::new((127.0, 127.0)),
        ];
        let mut curve = BezierCurve::from_points(points);
        
        assert_ne!(curve.evaluate(64.0), 64.0); // Должна быть нелинейная кривая
    }

    #[test]
    fn test_add_control_point() {
        let mut curve = BezierCurve::new();
        curve.add_control_point((64.0, 64.0));
        assert_eq!(curve.control_points.len(), 3);
    }

    #[test]
    fn test_remove_control_point() {
        let mut curve = BezierCurve::new();
        curve.add_control_point((64.0, 64.0));
        
        assert!(curve.remove_control_point(1)); // Можно удалить среднюю точку
        assert!(!curve.remove_control_point(0)); // Нельзя удалить первую точку
        
        assert_eq!(curve.control_points.len(), 2);
    }

    #[test]
    fn test_update_control_point() {
        let mut curve = BezierCurve::new();
        assert!(curve.update_control_point(0, (10.0, 10.0)));
        assert_eq!(curve.control_points[0].position, (10.0, 10.0));
    }

    #[test]
    fn test_reset_to_linear() {
        let points = vec![
            ControlPoint::new((0.0, 0.0)),
            ControlPoint::new((64.0, 32.0)),
            ControlPoint::new((127.0, 127.0)),
        ];
        let mut curve = BezierCurve::from_points(points);
        
        curve.reset_to_linear();
        assert_eq!(curve.evaluate(64.0), 64.0);
        assert_eq!(curve.control_points.len(), 2);
    }
}