//! Миграция кривых старого формата (до 2026-11: `control_points` в единицах
//! velocity 0…127) в [`CurveShape`] — точно, без потери формы.
//!
//! Старая математика (`BezierCurve::evaluate_directly`):
//! - 2 точки — прямая через них (вне отрезка — продолжение прямой), зажатая
//!   в [0, 127];
//! - 3 и больше — ОДИН кубический Безье с вершинами P₀…P₃ (первые 4 точки,
//!   при 3 точках P₃ = P₂); левее P₀.x — y(P₀), правее P₃.x — y(P₃).
//!
//! Перевод: узлы P₀/127 и P₃/127, `hout` = (P₁ − P₀)/127, `hin` = (P₂ − P₃)/127;
//! если P₀.x > 0 или P₃.x < 127 — горизонтальные участки до краёв. Точно,
//! когда x₁, x₂ ∈ [x₀, x₃] (тогда ручки не ужимаются); у старых данных это так,
//! пока крайние точки на x = 0 и 127 (точки были зажаты в 0…127). Формы, где
//! крайние точки перетащены внутрь и ручки торчат за них, функцией velocity не
//! были — для них перевод приближённый (ручки ужимаются, `normalize`).

use serde::{Deserialize, Serialize};

use super::shape::{CurveShape, Knot};

/// Точка старого формата (пресеты и `settings.json` до версии 2). Ручки
/// `handle_*` в старой математике не участвовали.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LegacyPoint {
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub handle_in_x: f32,
    #[serde(default)]
    pub handle_in_y: f32,
    #[serde(default)]
    pub handle_out_x: f32,
    #[serde(default)]
    pub handle_out_y: f32,
}

impl LegacyPoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y, handle_in_x: 0.0, handle_in_y: 0.0, handle_out_x: 0.0, handle_out_y: 0.0 }
    }
}

const S: f32 = 127.0;

/// Угловой узел без ручек (их ставит [`linear_handles`]).
fn corner(x: f32, y: f32) -> Knot {
    Knot { x, y, hin: [0.0, 0.0], hout: [0.0, 0.0], smooth: false }
}

/// Ручки по трети сегмента вдоль хорды — сегмент становится отрезком.
fn linear_handles(knots: &mut [Knot], i: usize) {
    let d = [(knots[i + 1].x - knots[i].x) / 3.0, (knots[i + 1].y - knots[i].y) / 3.0];
    knots[i].hout = d;
    knots[i + 1].hin = [-d[0], -d[1]];
}

/// Ломаная по узлам: все сегменты — отрезки.
fn polyline(pts: &[(f32, f32)]) -> CurveShape {
    let mut knots: Vec<Knot> = pts.iter().map(|&(x, y)| corner(x, y)).collect();
    for i in 0..knots.len() - 1 {
        linear_handles(&mut knots, i);
    }
    CurveShape { knots }
}

/// Старая кривая из 2 точек: прямая через них, зажатая в [0, 1]. Узлы — на
/// краях и там, где прямая пересекает y = 0 или y = 1 (там излом от клампа).
fn from_line(a: LegacyPoint, b: LegacyPoint) -> CurveShape {
    let (x1, y1, x2, y2) = (a.x / S, a.y / S, b.x / S, b.y / S);
    let k = if x2 != x1 { (y2 - y1) / (x2 - x1) } else { 0.0 };
    let f = |x: f32| y1 + k * (x - x1);
    let mut xs = vec![0.0f32, 1.0];
    if k != 0.0 {
        for level in [0.0f32, 1.0] {
            let xc = x1 + (level - y1) / k;
            if xc > 0.0 && xc < 1.0 {
                xs.push(xc);
            }
        }
    }
    xs.sort_by(f32::total_cmp);
    let pts: Vec<(f32, f32)> = xs.iter().map(|&x| (x, f(x).clamp(0.0, 1.0))).collect();
    polyline(&pts)
}

/// Старая кривая из 3+ точек: кубический Безье P₀…P₃ с полками до краёв.
fn from_cubic(p: [LegacyPoint; 4]) -> CurveShape {
    let u = |q: LegacyPoint| [q.x / S, q.y / S];
    let (p0, p1, p2, p3) = (u(p[0]), u(p[1]), u(p[2]), u(p[3]));
    let (shelf_lo, shelf_hi) = (p0[0] > 0.0, p3[0] < 1.0);
    let mut knots = Vec::with_capacity(4);
    if shelf_lo {
        knots.push(corner(0.0, p0[1]));
    }
    knots.push(Knot {
        x: p0[0],
        y: p0[1],
        hin: [0.0, 0.0],
        hout: [p1[0] - p0[0], p1[1] - p0[1]],
        smooth: false,
    });
    knots.push(Knot {
        x: p3[0],
        y: p3[1],
        hin: [p2[0] - p3[0], p2[1] - p3[1]],
        hout: [0.0, 0.0],
        smooth: false,
    });
    if shelf_hi {
        knots.push(corner(1.0, p3[1]));
    }
    if shelf_lo {
        linear_handles(&mut knots, 0);
    }
    if shelf_hi {
        let n = knots.len();
        linear_handles(&mut knots, n - 2);
    }
    let mut shape = CurveShape { knots };
    shape.normalize();
    shape
}

/// Старая кривая → форма (см. описание модуля). Меньше 2 точек — диагональ.
pub fn shape_from_legacy(points: &[LegacyPoint]) -> CurveShape {
    match points.len() {
        0 | 1 => CurveShape::linear(),
        2 => from_line(points[0], points[1]),
        n => {
            let p3 = if n == 3 { points[2] } else { points[3] };
            from_cubic([points[0], points[1], points[2], p3])
        }
    }
}

/// Старая математика дословно (`BezierCurve::evaluate_directly` и
/// `find_t_for_x`, удалены в 2026-11) — эталон для тестов миграции.
#[cfg(test)]
pub(crate) fn legacy_eval(points: &[LegacyPoint], x: f32) -> f32 {
    fn cubic(t: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
        let (t2, mt) = (t * t, 1.0 - t);
        let mt2 = mt * mt;
        mt2 * mt * p0 + 3.0 * mt2 * t * p1 + 3.0 * mt * t2 * p2 + t2 * t * p3
    }
    if points.len() < 2 {
        return x;
    }
    let y = if points.len() == 2 {
        let (x1, y1, x2, y2) = (points[0].x, points[0].y, points[1].x, points[1].y);
        let ratio = if x2 != x1 { (x - x1) / (x2 - x1) } else { 0.0 };
        y1 + (y2 - y1) * ratio
    } else {
        let mut p: Vec<(f32, f32)> = points.iter().map(|q| (q.x, q.y)).collect();
        while p.len() < 4 {
            p.push(*p.last().unwrap());
        }
        let (mut low, mut high) = (0.0f32, 1.0f32);
        let mut t = None;
        for _ in 0..20 {
            let mid = (low + high) / 2.0;
            let x_mid = cubic(mid, p[0].0, p[1].0, p[2].0, p[3].0);
            if (x_mid - x).abs() < 0.001 {
                t = Some(mid);
                break;
            }
            if x_mid < x {
                low = mid;
            } else {
                high = mid;
            }
        }
        let t = t.unwrap_or((low + high) / 2.0);
        cubic(t, p[0].1, p[1].1, p[2].1, p[3].1)
    };
    y.clamp(0.0, 127.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(v: &[(f32, f32)]) -> Vec<LegacyPoint> {
        v.iter().map(|&(x, y)| LegacyPoint::new(x, y)).collect()
    }

    /// Новая форма против старой математики на всей шкале (шаг 1/8 velocity).
    pub(crate) fn max_diff(points: &[LegacyPoint]) -> f32 {
        let shape = shape_from_legacy(points);
        (0..=127 * 8)
            .map(|j| {
                let x = j as f32 / 8.0;
                (shape.eval(x / S) * S - legacy_eval(points, x)).abs()
            })
            .fold(0.0, f32::max)
    }

    #[test]
    fn two_points_line_with_clamp() {
        // Обычная диагональ, сдвинутая прямая, прямая с клампом сверху и снизу.
        for v in [
            &[(0.0, 0.0), (127.0, 127.0)][..],
            &[(0.0, 54.45456), (127.0, 127.0)],
            &[(20.0, 0.0), (80.0, 127.0)],
            &[(10.0, 100.0), (60.0, 20.0)],
            &[(40.0, 30.0), (40.0, 90.0)],
        ] {
            let p = pts(v);
            assert!(max_diff(&p) < 0.01, "{v:?}: {}", max_diff(&p));
        }
    }

    #[test]
    fn cubic_with_shelves_and_three_points() {
        for v in [
            &[(0.0, 0.0), (42.0, 32.0), (85.0, 95.0), (127.0, 127.0)][..],
            &[(0.0, 0.0), (25.518707, 88.82972), (127.0, 127.0)],
            &[(15.0, 10.0), (30.0, 90.0), (90.0, 20.0), (110.0, 120.0)],
            // 5+ точек: звучали только первые 4.
            &[(0.0, 0.0), (20.0, 60.0), (64.0, 64.0), (100.0, 30.0), (127.0, 127.0)],
        ] {
            let p = pts(v);
            assert!(max_diff(&p) < 0.01, "{v:?}: {}", max_diff(&p));
        }
    }

    #[test]
    fn fewer_than_two_points_is_linear() {
        assert_eq!(shape_from_legacy(&[]), CurveShape::linear());
        assert_eq!(shape_from_legacy(&pts(&[(3.0, 4.0)])), CurveShape::linear());
    }
}

/// Пресеты и настройки пользователя на 2026-11-20 (копии из
/// `~/.config/vst_midi_curves`): новая форма совпадает со старой математикой.
#[cfg(test)]
mod fixtures {
    use super::tests::max_diff;
    use super::LegacyPoint;

    fn points(v: &serde_json::Value) -> Vec<LegacyPoint> {
        serde_json::from_value(v.clone()).unwrap()
    }

    #[test]
    fn user_presets_and_settings_migrate_exactly() {
        let presets = [
            include_str!("../../tests/fixtures/legacy/p2_lower.json"),
            include_str!("../../tests/fixtures/legacy/p2_upper.json"),
            include_str!("../../tests/fixtures/legacy/pianoteq.json"),
            include_str!("../../tests/fixtures/legacy/pianoteqOff.json"),
            include_str!("../../tests/fixtures/legacy/soft_s_curve.json"),
        ];
        let mut curves: Vec<Vec<LegacyPoint>> = presets
            .iter()
            .map(|t| points(&serde_json::from_str::<serde_json::Value>(t).unwrap()["control_points"]))
            .collect();
        let settings: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/legacy/settings.json")).unwrap();
        curves.push(points(&settings["note_on_points"]));
        curves.push(points(&settings["note_off_points"]));
        for (i, c) in curves.iter().enumerate() {
            let d = max_diff(c);
            assert!(d < 0.01, "кривая {i}: расхождение {d} velocity");
        }
    }
}
