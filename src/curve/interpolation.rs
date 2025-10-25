/// Математические функции для работы с кривыми Безье

/// Вычисляет значение кубической кривой Безье в точке t
/// 
/// # Формула:
/// B(t) = (1-t)³P₀ + 3(1-t)²tP₁ + 3(1-t)t²P₂ + t³P₃
/// 
/// # Параметры:
/// - t: параметр от 0.0 до 1.0
/// - p0, p1, p2, p3: контрольные точки кривой
/// 
/// # Возвращает:
/// Значение кривой в точке t
pub fn cubic_bezier(t: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let mt3 = mt2 * mt;
    
    mt3 * p0 + 3.0 * mt2 * t * p1 + 3.0 * mt * t2 * p2 + t3 * p3
}

/// Находит параметр t для заданного значения x методом бинарного поиска
/// 
/// # Параметры:
/// - curve_points: вектор контрольных точек кривой
/// - x_target: целевое значение x
/// 
/// # Возвращает:
/// Значение параметра t
pub fn find_x_for_t(curve_points: &[(f32, f32)], x_target: f32) -> f32 {
    let mut low = 0.0;
    let mut high = 1.0;
    
    for _ in 0..20 { // 20 итераций для точности
        let mid = (low + high) / 2.0;
        let mid_value = evaluate_curve_at(curve_points, mid).0; // Берем только x координату
        
        if (mid_value - x_target).abs() < 0.001 {
            return mid;
        }
        
        if mid_value < x_target {
            low = mid;
        } else {
            high = mid;
        }
    }
    
    (low + high) / 2.0
}

/// Вспомогательная функция для вычисления координат кривой в точке t
fn evaluate_curve_at(curve_points: &[(f32, f32)], t: f32) -> (f32, f32) {
    if curve_points.len() == 2 {
        // Линейная интерполяция между двумя точками
        let (x1, y1) = curve_points[0];
        let (x2, y2) = curve_points[1];
        let x = x1 + (x2 - x1) * t;
        let y = y1 + (y2 - y1) * t;
        (x, y)
    } else {
        // Для большего количества точек используем первые 4
        let p0 = curve_points[0];
        let p1 = curve_points.get(1).unwrap_or(&p0);
        let p2 = curve_points.get(2).unwrap_or(&p1);
        let p3 = curve_points.get(3).unwrap_or(&p2);
        
        let x = cubic_bezier(t, p0.0, p1.0, p2.0, p3.0);
        let y = cubic_bezier(t, p0.1, p1.1, p2.1, p3.1);
        (x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cubic_bezier_linear() {
        // Тест линейной кривой
        let result = cubic_bezier(0.0, 0.0, 33.33, 66.67, 100.0);
        assert!((result - 0.0).abs() < 0.001);
        
        let result = cubic_bezier(1.0, 0.0, 33.33, 66.67, 100.0);
        assert!((result - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_cubic_bezier_middle() {
        // Тест средней точки
        let result = cubic_bezier(0.5, 0.0, 0.0, 100.0, 100.0);
        // Для симметричной кривой с контрольными точками на краях
        assert!((result - 50.0).abs() < 1.0);
    }
}