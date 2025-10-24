//! Функции интерполяции для кривых Безье

/// Кубическая кривая Безье: B(t) = (1-t)³P₀ + 3(1-t)²tP₁ + 3(1-t)t²P₂ + t³P₃
pub fn cubic_bezier(t: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let mt3 = mt2 * mt;
    
    mt3 * p0 + 3.0 * mt2 * t * p1 + 3.0 * mt * t2 * p2 + t3 * p3
}

/// Линейная интерполяция между двумя точками
pub fn lerp(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}

/// Интерполяция с использованием метода Ньютона-Рафсона для нахождения t по x
pub fn find_t_for_x<F>(x_target: f32, eval_x: F, max_iterations: usize) -> f32 
where 
    F: Fn(f32) -> f32,
{
    let mut t = 0.5;
    let tolerance = 1e-6;
    
    for _ in 0..max_iterations {
        let x_current = eval_x(t);
        let error = x_current - x_target;
        
        if error.abs() < tolerance {
            break;
        }
        
        // Производная (приближенная)
        let t_plus = t + 0.001;
        let x_plus = eval_x(t_plus);
        let derivative = (x_plus - x_current) / 0.001;
        
        if derivative.abs() > 1e-10 {
            t -= error / derivative;
            t = t.clamp(0.0, 1.0);
        } else {
            break;
        }
    }
    
    t
}

/// Сплайн-интерполяция для сглаживания
pub fn cubic_spline_interpolation(points: &[(f32, f32)], x: f32) -> f32 {
    if points.is_empty() {
        return 0.0;
    }
    
    if x <= points[0].0 {
        return points[0].1;
    }
    
    if x >= points[points.len() - 1].0 {
        return points[points.len() - 1].1;
    }
    
    // Находим сегмент, в котором находится x
    for i in 0..points.len() - 1 {
        if x >= points[i].0 && x <= points[i + 1].0 {
            let t = (x - points[i].0) / (points[i + 1].0 - points[i].0);
            return lerp(points[i].1, points[i + 1].1, t);
        }
    }
    
    0.0
}