/// Основная структура для работы с кривыми Безье
use crate::curve::{ControlPoint, cubic_bezier};

#[derive(Debug, Clone)]
pub struct BezierCurve {
    /// Контрольные точки кривой
    pub control_points: Vec<ControlPoint>,
    /// Кэшированная таблица значений для быстрого доступа (LUT)
    cached_lut: Vec<(f32, f32)>,
    /// Флаг, указывающий нужно ли обновить кэш
    pub dirty: bool,
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
        
        let mut y_value = 0.0;
        
        if self.control_points.len() == 2 {
            // Линейная интерполяция
            let (x1, y1) = self.control_points[0].position;
            let (x2, y2) = self.control_points[1].position;
            let ratio = if x2 != x1 { (x - x1) / (x2 - x1) } else { 0.0 };
            y_value = y1 + (y2 - y1) * ratio;
        } else {
            // Подготавливаем точки для расчета
            let mut points: Vec<(f32, f32)> = Vec::new();
            for point in &self.control_points {
                points.push(point.position);
            }
            
            // Добавляем фиктивные точки если нужно
            while points.len() < 4 {
                points.push(points.last().copied().unwrap_or((127.0, 127.0)));
            }
            
            // Находим t для заданного x
            let t = self.find_t_for_x(x, &points);
            
            // Вычисляем Y координату используя правильный X для расчета t
            let y = cubic_bezier(t, points[0].1, points[1].1, points[2].1, points[3].1);
            y_value = y;
        }
        
        y_value.clamp(0.0, 127.0)
    }

    /// Находит параметр t для заданного x методом бинарного поиска
    fn find_t_for_x(&self, target_x: f32, points: &[(f32, f32)]) -> f32 {
        let mut low = 0.0;
        let mut high = 1.0;
        
        for _ in 0..20 { // 20 итераций для точности
            let mid = (low + high) / 2.0;
            
            // Вычисляем X координату в точке mid
            let x_mid = cubic_bezier(mid, points[0].0, points[1].0, points[2].0, points[3].0);
            
            if (x_mid - target_x).abs() < 0.001 {
                return mid;
            }
            
            if x_mid < target_x {
                low = mid;
            } else {
                high = mid;
            }
        }
        
        (low + high) / 2.0
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

    /// Проверяет является ли кривая линейной (y = x)
    pub fn is_linear(&self) -> bool {
        // Кривая линейная если у неё только 2 точки в позициях (0,0) и (127,127)
        if self.control_points.len() != 2 {
            return false;
        }
        
        let p0 = self.control_points[0].position;
        let p1 = self.control_points[1].position;
        
        // Проверяем что точки в правильных позициях (с небольшой погрешностью)
        const EPSILON: f32 = 0.1;
        (p0.0 - 0.0).abs() < EPSILON &&
        (p0.1 - 0.0).abs() < EPSILON &&
        (p1.0 - 127.0).abs() < EPSILON &&
        (p1.1 - 127.0).abs() < EPSILON
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

    /// Возвращает кэшированные значения (обновляет кэш при необходимости)
    pub fn get_cached_values(&mut self) -> &[(f32, f32)] {
        // Принудительно обновляем кэш при каждом запросе для отрисовки
        self.ensure_cache_updated();
        &self.cached_lut
    }
    
    /// Вычисляет значение кривой в нормализованном пространстве (0.0-1.0)
    /// Это позволяет работать с любым диапазоном значений без потери точности
    /// ВАЖНО: Использует прямое вычисление БЕЗ кэша для сохранения 14-битной точности
    pub fn evaluate_normalized(&mut self, normalized_x: f64) -> f64 {
        // Масштабируем нормализованный вход к диапазону кривой
        let curve_x = (normalized_x * 127.0) as f32;
        
        // Получаем значение из кривой НАПРЯМУЮ (без кэша для точности)
        let curve_y = self.evaluate_directly(curve_x);
        
        // Нормализуем выход обратно к 0.0-1.0
        (curve_y as f64) / 127.0
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
    use crate::curve::{BezierCurve, ControlPoint};

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
    
    #[test]
    fn test_cached_values() {
        let mut curve = BezierCurve::new();
        
        // Проверяем что кэш не пустой
        let cached = curve.get_cached_values();
        assert!(!cached.is_empty());
        assert_eq!(cached.len(), 128); // 0..=127
        
        // Проверяем что кэш содержит правильные значения
        assert_eq!(cached[0], (0.0, 0.0));
        assert_eq!(cached[127], (127.0, 127.0));
    }
}