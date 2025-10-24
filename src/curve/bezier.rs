//! Реализация кривой Безье для обработки MIDI velocity

use kurbo::{BezPath, Point};
use super::ControlPoint;

#[derive(Debug, Clone)]
pub struct BezierCurve {
    control_points: Vec<ControlPoint>,
    cached_lut: Vec<Point>, // Look-up table для оптимизации
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
        let x_normalized = x * 127.0;
        
        // Использование LUT для быстрого доступа
        if self.cached_lut.is_empty() {
            return x; // Fallback к линейной кривой
        }

        // Поиск ближайших точек в LUT
        let index = (x_normalized * (self.cached_lut.len() - 1) as f32 / 127.0) as usize;
        let index = index.min(self.cached_lut.len() - 1);

        if index == self.cached_lut.len() - 1 {
            return self.cached_lut[index].y / 127.0;
        }

        // Линейная интерполяция между точками LUT
        let p1 = self.cached_lut[index];
        let p2 = self.cached_lut[index + 1];
        
        let t = (x_normalized - p1.x) / (p2.x - p1.x);
        let y = p1.y + t * (p2.y - p1.y);
        
        y / 127.0
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
            let t = self.find_t_for_x(x);
            let y = self.evaluate_bezier(t);
            self.cached_lut.push(Point::new(x, y));
        }
    }

    /// Находит параметр t для заданного x с помощью бинарного поиска
    fn find_t_for_x(&self, x_target: f32) -> f32 {
        let mut low = 0.0;
        let mut high = 1.0;
        let mut t = 0.5;
        
        for _ in 0..16 { // Ограничиваем количество итераций
            let x_current = self.evaluate_bezier_x(t);
            if (x_current - x_target).abs() < 0.001 {
                break;
            }
            
            if x_current < x_target {
                low = t;
            } else {
                high = t;
            }
            t = (low + high) / 2.0;
        }
        
        t
    }

    /// Вычисление x координаты кривой Безье для параметра t
    fn evaluate_bezier_x(&self, t: f32) -> f32 {
        if self.control_points.len() < 2 {
            return 0.0;
        }

        let n = self.control_points.len() - 1;
        let mut x = 0.0;
        
        for i in 0..=n {
            let binomial = Self::binomial_coefficient(n, i) as f32;
            let term = binomial * 
                      (1.0 - t).powi((n - i) as i32) * 
                      t.powi(i as i32) * 
                      self.control_points[i].position.x;
            x += term;
        }
        
        x
    }

    /// Вычисление y координаты кривой Безье для параметра t
    fn evaluate_bezier(&self, t: f32) -> f32 {
        if self.control_points.len() < 2 {
            return 0.0;
        }

        let n = self.control_points.len() - 1;
        let mut y = 0.0;
        
        for i in 0..=n {
            let binomial = Self::binomial_coefficient(n, i) as f32;
            let term = binomial * 
                      (1.0 - t).powi((n - i) as i32) * 
                      t.powi(i as i32) * 
                      self.control_points[i].position.y;
            y += term;
        }
        
        y
    }

    /// Биномиальный коэффициент C(n, k)
    fn binomial_coefficient(n: usize, k: usize) -> usize {
        if k > n {
            return 0;
        }
        
        let mut result = 1;
        for i in 1..=k {
            result = result * (n - i + 1) / i;
        }
        result
    }
}

impl Default for BezierCurve {
    fn default() -> Self {
        Self::new()
    }
}