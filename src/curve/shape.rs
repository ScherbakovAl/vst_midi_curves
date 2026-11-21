//! Форма кривой velocity — перенос `CurveShape` из piano_sim
//! (`src/gui/vel_curve.rs`, тот же автор) почти дословно.
//!
//! Кривая живёт в единичном квадрате: x — входная velocity 0…1, y — выходная
//! 0…1. Узлы по возрастанию x, крайние — на x = 0 и 1; кривая ПРОХОДИТ через
//! узлы, между соседними узлами — кубический Безье
//!
//!   B(t) = (1−t)³·P₀ + 3(1−t)²t·P₁ + 3(1−t)t²·P₂ + t³·P₃,  t ∈ [0, 1],
//!
//! где P₀ — узел i, P₁ = P₀ + `hout`ᵢ, P₂ = P₃ + `hin`ᵢ₊₁, P₃ — узел i+1
//! ([`CurveShape::seg`]). Ручки по x ужимаются в ширину сегмента
//! ([`fit_handle`]) ⇒ x(t) монотонна ⇒ кривая всегда функция velocity.

use serde::{Deserialize, Serialize};

/// Наименьший зазор по x между соседними узлами кривой.
pub const KNOT_GAP: f32 = 1.0e-3;

/// Узел кривой с ручками Безье. Ручки — смещения от узла: входящая (`hin`,
/// x ≤ 0) и исходящая (`hout`, x ≥ 0). Храним их «как поставили»: по x ручка
/// при вычислении ужимается в ширину своего сегмента (см. [`CurveShape::h_out`]),
/// поэтому сдвиг узла туда-обратно форму не портит.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Knot {
    pub x: f32,
    pub y: f32,
    pub hin: [f32; 2],
    pub hout: [f32; 2],
    /// Гладкий узел: ручки на одной прямой (Alt+драг ручки ломает).
    pub smooth: bool,
}

/// Кривая в единичном квадрате: узлы по возрастанию x, крайние — на x = 0 и 1.
/// Ручки по x не выходят за свой сегмент ⇒ x(t) каждого сегмента монотонна
/// (производная в базисе Бернштейна неотрицательна при x₁, x₂ ∈ [x₀, x₃]) —
/// кривая остаётся ФУНКЦИЕЙ velocity при любом изгибе.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurveShape {
    pub knots: Vec<Knot>,
}

impl Default for CurveShape {
    fn default() -> Self {
        Self::linear()
    }
}

pub type P2 = [f32; 2];

pub(crate) fn lerp2(a: P2, b: P2, t: f32) -> P2 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

pub(crate) fn add2(a: P2, b: P2) -> P2 {
    [a[0] + b[0], a[1] + b[1]]
}

pub(crate) fn sub2(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

/// Ужать ручку по x в [0, w] (знак x — по стороне: `sign` = +1 исходящая,
/// −1 входящая), сохраняя направление: вектор масштабируется целиком.
pub(crate) fn fit_handle(h: P2, w: f32, sign: f32) -> P2 {
    let hx = (h[0] * sign).max(0.0);
    let h = [hx * sign, h[1]];
    if hx > w {
        let k = w / hx;
        [h[0] * k, h[1] * k]
    } else {
        h
    }
}

/// Точка кубического Безье по контрольным точкам.
pub(crate) fn bez(p: &[P2; 4], t: f32) -> P2 {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [
        a * p[0][0] + b * p[1][0] + c * p[2][0] + d * p[3][0],
        a * p[0][1] + b * p[1][1] + c * p[2][1] + d * p[3][1],
    ]
}

/// Параметр t ∈ [`t0`, 1] точки сегмента `p` с абсциссой `x`; x(t) монотонна
/// (см. [`CurveShape`]), `t0` — нижняя граница корня (при проходе по
/// возрастающим x — t прошлой точки). Ньютон по x(t) = a·t³ + b·t² + c·t + x₀
/// (степенной вид Безье, в f64) с вилкой [lo, hi]: шаг за вилку или по плоскому
/// месту (x'(t) ≤ 0) — бисекция, поэтому сходится всегда.
pub(crate) fn solve_t(p: &[P2; 4], x: f32, t0: f32) -> f32 {
    let (x0, x1, x2, x3) = (p[0][0] as f64, p[1][0] as f64, p[2][0] as f64, p[3][0] as f64);
    let c = 3.0 * (x1 - x0);
    let b = 3.0 * (x2 - 2.0 * x1 + x0);
    let a = x3 - x0 - c - b;
    let x = x as f64;
    let (mut lo, mut hi) = (t0.clamp(0.0, 1.0) as f64, 1.0f64);
    let mut t = lo;
    for _ in 0..60 {
        let f = ((a * t + b) * t + c) * t + x0 - x;
        if f.abs() <= 1.0e-10 {
            break;
        }
        if f < 0.0 {
            lo = t;
        } else {
            hi = t;
        }
        if hi - lo <= 1.0e-9 {
            break;
        }
        let d = (3.0 * a * t + 2.0 * b) * t + c;
        let tn = t - f / d;
        t = if d > 0.0 && tn > lo && tn < hi { tn } else { 0.5 * (lo + hi) };
    }
    t as f32
}

impl CurveShape {
    /// Диагональ y = x.
    pub fn linear() -> Self {
        let h = 1.0 / 3.0;
        Self {
            knots: vec![
                Knot { x: 0.0, y: 0.0, hin: [0.0, 0.0], hout: [h, h], smooth: true },
                Knot { x: 1.0, y: 1.0, hin: [-h, -h], hout: [0.0, 0.0], smooth: true },
            ],
        }
    }

    /// Действующая исходящая ручка узла `i` (ужата в ширину сегмента справа).
    pub fn h_out(&self, i: usize) -> P2 {
        let k = &self.knots;
        let w = if i + 1 < k.len() { k[i + 1].x - k[i].x } else { 0.0 };
        fit_handle(k[i].hout, w, 1.0)
    }

    /// Действующая входящая ручка узла `i` (ужата в ширину сегмента слева).
    pub fn h_in(&self, i: usize) -> P2 {
        let k = &self.knots;
        let w = if i > 0 { k[i].x - k[i - 1].x } else { 0.0 };
        fit_handle(k[i].hin, w, -1.0)
    }

    pub fn pt(&self, i: usize) -> P2 {
        [self.knots[i].x, self.knots[i].y]
    }

    /// Контрольные точки сегмента между узлами `i` и `i+1`.
    pub fn seg(&self, i: usize) -> [P2; 4] {
        let (a, b) = (self.pt(i), self.pt(i + 1));
        [a, add2(a, self.h_out(i)), add2(b, self.h_in(i + 1)), b]
    }

    /// Сегмент, на котором лежит x, и параметр t точки кривой с этим x.
    pub fn locate(&self, x: f32) -> (usize, f32) {
        let x = x.clamp(0.0, 1.0);
        let n = self.knots.len();
        let i = self.knots[1..n - 1].iter().take_while(|k| k.x <= x).count();
        let p = self.seg(i);
        if p[3][0] - p[0][0] <= 0.0 {
            return (i, 0.0);
        }
        // x(t) монотонна (см. CurveShape) — бисекция надёжна и на плоских
        // участках производной, где Ньютон бы споткнулся.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..32 {
            let mid = 0.5 * (lo + hi);
            if bez(&p, mid)[0] < x {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        (i, 0.5 * (lo + hi))
    }

    /// y(x) кривой, зажатый в [0, 1] (перелёт ручек за край — по краю).
    pub fn eval(&self, x: f32) -> f32 {
        let (i, t) = self.locate(x);
        bez(&self.seg(i), t)[1].clamp(0.0, 1.0)
    }

    /// Таблица y по равномерной сетке x_j = j/(len−1) (len ≥ 2) — то же, что
    /// `eval` в каждой точке, но за один проход: x растёт ⇒ сегмент и t внутри
    /// сегмента не убывают, корень ищется от t прошлой точки ([`solve_t`]).
    pub fn fill_lut(&self, out: &mut [f32]) {
        let n = self.knots.len();
        let last = (out.len() - 1) as f32;
        let mut i = 0;
        let mut p = self.seg(0);
        let mut t = 0.0f32;
        for (j, o) in out.iter_mut().enumerate() {
            let x = j as f32 / last;
            while i + 2 < n && self.knots[i + 1].x <= x {
                i += 1;
                p = self.seg(i);
                t = 0.0;
            }
            if p[3][0] - p[0][0] > 0.0 {
                t = solve_t(&p, x, t);
            }
            *o = bez(&p, t)[1].clamp(0.0, 1.0);
        }
    }

    /// Привести форму в допустимую: ≥2 конечных узла по возрастанию x с
    /// зазором, крайние на 0 и 1, y в [0, 1]. Чинит формы из файлов.
    pub fn normalize(&mut self) {
        let finite = self.knots.iter().all(|k| {
            [k.x, k.y, k.hin[0], k.hin[1], k.hout[0], k.hout[1]].iter().all(|v| v.is_finite())
        });
        if self.knots.len() < 2 || !finite {
            *self = Self::linear();
            return;
        }
        self.knots.sort_by(|a, b| a.x.total_cmp(&b.x));
        self.clamp_knots();
    }

    /// Порядок узлов (без сортировки) и y в [0, 1] — см. [`Self::clamp_x`].
    fn clamp_knots(&mut self) {
        self.clamp_x();
        for k in &mut self.knots {
            k.y = k.y.clamp(0.0, 1.0);
        }
    }

    /// Порядок узлов по x (без сортировки): крайние на 0 и 1, внутренние —
    /// x_i ∈ [x_{i−1} + KNOT_GAP, 1 − KNOT_GAP·(n−1−i)].
    pub(crate) fn clamp_x(&mut self) {
        let n = self.knots.len();
        self.knots[0].x = 0.0;
        self.knots[n - 1].x = 1.0;
        for i in 1..n - 1 {
            let lo = self.knots[i - 1].x + KNOT_GAP;
            let hi = 1.0 - KNOT_GAP * (n - 1 - i) as f32;
            self.knots[i].x = self.knots[i].x.clamp(lo, hi.max(lo));
        }
    }

    /// Вставить узел в точку кривой с абсциссой `x`, НЕ меняя форму
    /// (разбиение сегмента де Кастельжо). Индекс нового узла; `None` — точка
    /// вплотную к существующему узлу.
    pub fn insert_at(&mut self, x: f32) -> Option<usize> {
        let (i, t) = self.locate(x);
        self.split(i, t)
    }

    /// Вставить узел в сегмент `i` в точке с параметром `t`, НЕ меняя форму
    /// (де Кастельжо). `None` — новый узел ближе `KNOT_GAP` к соседнему.
    pub fn split(&mut self, i: usize, t: f32) -> Option<usize> {
        if i + 1 >= self.knots.len() {
            return None;
        }
        let m = bez(&self.seg(i), t);
        let gap_ok = m[0] - self.knots[i].x >= KNOT_GAP && self.knots[i + 1].x - m[0] >= KNOT_GAP;
        gap_ok.then(|| self.split_forced(i, t))
    }

    /// То же без проверки зазора: узел вставляется всегда, слишком близкий
    /// сдвигается на `KNOT_GAP` (форма почти не меняется). Для вставки узла
    /// сразу во все ключевые ноты карты.
    pub(crate) fn split_forced(&mut self, i: usize, t: f32) -> usize {
        let p = self.seg(i);
        let (p01, p12, p23) = (lerp2(p[0], p[1], t), lerp2(p[1], p[2], t), lerp2(p[2], p[3], t));
        let (p012, p123) = (lerp2(p01, p12, t), lerp2(p12, p23, t));
        let m = lerp2(p012, p123, t);
        self.knots[i].hout = sub2(p01, p[0]);
        self.knots[i + 1].hin = sub2(p23, p[3]);
        self.knots.insert(
            i + 1,
            Knot { x: m[0], y: m[1], hin: sub2(p012, m), hout: sub2(p123, m), smooth: true },
        );
        self.clamp_x();
        i + 1
    }

    /// Удалить внутренний узел (крайние неудаляемы).
    pub fn remove(&mut self, i: usize) {
        if i > 0 && i + 1 < self.knots.len() {
            self.knots.remove(i);
        }
    }

    /// Сдвинуть узел в (x, y): крайние ходят только по y, внутренние — между
    /// соседями (с зазором). Ручки едут вместе с узлом.
    pub fn move_knot(&mut self, i: usize, x: f32, y: f32) {
        let n = self.knots.len();
        if i > 0 && i + 1 < n {
            let lo = self.knots[i - 1].x + KNOT_GAP;
            let hi = self.knots[i + 1].x - KNOT_GAP;
            self.knots[i].x = x.clamp(lo, hi.max(lo));
        }
        self.knots[i].y = y.clamp(0.0, 1.0);
    }

    /// Поставить ручку узла `i` в точку `pos` (`out` — исходящая). У гладкого
    /// узла противоположная ручка поворачивается на ту же прямую, сохраняя
    /// длину; `break_smooth` (Alt) делает узел угловым.
    pub fn set_handle(&mut self, i: usize, out: bool, pos: P2, break_smooth: bool) {
        let k = &mut self.knots[i];
        let mut v = sub2(pos, [k.x, k.y]);
        v[0] = if out { v[0].max(0.0) } else { v[0].min(0.0) };
        if break_smooth {
            k.smooth = false;
        }
        let other = if out { k.hin } else { k.hout };
        if out {
            k.hout = v;
        } else {
            k.hin = v;
        }
        let len_v = v[0].hypot(v[1]);
        if k.smooth && len_v > 1.0e-6 {
            let len_o = other[0].hypot(other[1]);
            let mirrored = [-v[0] / len_v * len_o, -v[1] / len_v * len_o];
            if out {
                k.hin = mirrored;
            } else {
                k.hout = mirrored;
            }
        }
    }

    /// Симметричная ручка (ЛКМ-драг ручки): противоположная ручка — точное зеркало
    /// через узел (та же длина, C1 в узле), узел становится гладким. Ход ручки
    /// по x ужат в ширину УЖЕГО из двух сегментов — иначе `h_in`/`h_out`
    /// ужали бы одну сторону и симметрия пропала бы. У крайнего узла второй
    /// ручки нет — обычная `set_handle`.
    pub fn set_handle_sym(&mut self, i: usize, out: bool, pos: P2) {
        let n = self.knots.len();
        if i == 0 || i + 1 == n {
            self.set_handle(i, out, pos, false);
            return;
        }
        let w = (self.knots[i + 1].x - self.knots[i].x).min(self.knots[i].x - self.knots[i - 1].x);
        let k = &mut self.knots[i];
        let sign = if out { 1.0 } else { -1.0 };
        let v = fit_handle(sub2(pos, [k.x, k.y]), w, sign);
        k.smooth = true;
        if out {
            (k.hout, k.hin) = (v, [-v[0], -v[1]]);
        } else {
            (k.hin, k.hout) = (v, [-v[0], -v[1]]);
        }
    }

    /// Переключить гладкость узла. Ставший гладким узел выравнивает входящую
    /// ручку по исходящей (длины сохраняются).
    pub fn toggle_smooth(&mut self, i: usize) {
        let k = &mut self.knots[i];
        k.smooth = !k.smooth;
        if k.smooth {
            let (o, len_i) = (k.hout, k.hin[0].hypot(k.hin[1]));
            let len_o = o[0].hypot(o[1]);
            if len_o > 1.0e-6 {
                k.hin = [-o[0] / len_o * len_i, -o[1] / len_o * len_i];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_is_identity() {
        let c = CurveShape::linear();
        for j in 0..=100 {
            let x = j as f32 / 100.0;
            assert!((c.eval(x) - x).abs() < 1e-5, "x={x} y={}", c.eval(x));
        }
    }

    #[test]
    fn insert_keeps_shape() {
        let mut c = CurveShape::linear();
        c.set_handle(0, true, [0.1, 0.9], false);
        let before: Vec<f32> = (0..=50).map(|j| c.eval(j as f32 / 50.0)).collect();
        let i = c.insert_at(0.4);
        assert_eq!(i, Some(1));
        for (j, y0) in before.iter().enumerate() {
            let y1 = c.eval(j as f32 / 50.0);
            assert!((y1 - y0).abs() < 1e-4, "j={j}: {y0} → {y1}");
        }
    }

    #[test]
    fn sym_handle_mirrors_effective_handles() {
        // Узел на 0.3: сегменты 0.3 и 0.7; ручку тянут далеко вправо — обе
        // действующие ручки равны и противоположны, узел гладкий.
        let mut c = CurveShape::linear();
        let i = c.insert_at(0.3).unwrap();
        c.knots[i].smooth = false;
        c.set_handle_sym(i, true, [0.3 + 0.6, 0.3 + 0.2]);
        let (ho, hi) = (c.h_out(i), c.h_in(i));
        assert!(c.knots[i].smooth);
        assert!((ho[0] + hi[0]).abs() < 1e-6 && (ho[1] + hi[1]).abs() < 1e-6, "{ho:?} {hi:?}");
        assert!((ho[0] - 0.3).abs() < 1e-6, "ход ужат в узкий сегмент: {ho:?}");
    }

    #[test]
    fn extreme_handles_keep_x_monotone() {
        // Ручки, вытянутые далеко за сегмент, ужимаются: x(t) монотонна,
        // кривая — функция velocity.
        let mut c = CurveShape::linear();
        c.knots[0].hout = [5.0, 3.0];
        c.knots[1].hin = [-7.0, -0.2];
        let p = c.seg(0);
        let mut prev = -1.0f32;
        for j in 0..=1000 {
            let x = bez(&p, j as f32 / 1000.0)[0];
            assert!(x >= prev - 1e-6, "x(t) убывает на j={j}");
            prev = x;
        }
    }

    /// Один проход `fill_lut` даёт то же, что `eval` в каждой точке сетки, —
    /// и на формах с плоскими местами x(t) (ручки нулевой длины, «ступенька»,
    /// ручки за краем сегмента). На почти вертикальных участках эталон сам
    /// точен лишь до шага f32 по x — сверка с коридором eval(x ± 1 ulp).
    #[test]
    fn fill_lut_matches_eval() {
        let mut shapes = vec![CurveShape::linear()];
        let mut step = CurveShape::linear();
        step.knots[0].hout = [0.5, 0.0];
        step.knots[1].hin = [-0.5, 0.0];
        shapes.push(step);
        let mut flat = CurveShape::linear();
        flat.knots[0].hout = [0.0, 0.0];
        flat.knots[1].hin = [0.0, 0.0];
        shapes.push(flat);
        let mut steep = CurveShape::linear();
        steep.knots[0].hout = [0.02, 0.6];
        steep.knots[1].hin = [-0.9, -0.05];
        shapes.push(steep);
        let mut wild = CurveShape::linear();
        for (x, y) in [(0.2, 0.9), (0.21, 0.1), (0.6, 0.6), (0.9, 0.0)] {
            let i = wild.insert_at(x).unwrap();
            wild.move_knot(i, x, y);
        }
        wild.knots[1].hout = [3.0, -1.0];
        wild.knots[2].hin = [-4.0, 2.0];
        wild.knots[3].hout = [0.0, 0.3];
        shapes.push(wild);
        for s in &shapes {
            for len in [2, 257, 1025] {
                let mut lut = vec![0.0f32; len];
                s.fill_lut(&mut lut);
                for (j, &y) in lut.iter().enumerate() {
                    let x = j as f32 / (len - 1) as f32;
                    let (a, b) = (s.eval(x - 1.2e-7), s.eval(x + 1.2e-7));
                    let (lo, hi) = (a.min(b).min(s.eval(x)) - 1e-6, a.max(b).max(s.eval(x)) + 1e-6);
                    assert!((lo..=hi).contains(&y), "j={j}/{len}: {y} вне [{lo}, {hi}], форма {s:?}");
                }
            }
        }
    }

    #[test]
    fn normalize_repairs_broken_shapes() {
        // Узлы не по порядку, края не на 0/1, y вне квадрата, NaN — чинится.
        let mut c = CurveShape::linear();
        c.knots[0].x = 0.3;
        c.knots[1].x = 0.1;
        c.knots[1].y = 4.0;
        c.normalize();
        assert_eq!(c.knots[0].x, 0.0);
        assert_eq!(c.knots[1].x, 1.0);
        assert!(c.knots.iter().all(|k| (0.0..=1.0).contains(&k.y)));
        let mut bad = CurveShape::linear();
        bad.knots[0].hout[1] = f32::NAN;
        bad.normalize();
        assert_eq!(bad, CurveShape::linear());
    }

    #[test]
    fn edge_knots_move_only_vertically_and_remove_keeps_ends() {
        let mut c = CurveShape::linear();
        c.move_knot(0, 0.5, 0.2);
        assert_eq!(c.pt(0), [0.0, 0.2]);
        c.remove(0);
        c.remove(1);
        assert_eq!(c.knots.len(), 2);
        let i = c.insert_at(0.5).unwrap();
        c.move_knot(i, 2.0, 0.5);
        assert!(c.knots[i].x <= 1.0 - KNOT_GAP);
        c.remove(i);
        assert_eq!(c.knots.len(), 2);
    }
}
