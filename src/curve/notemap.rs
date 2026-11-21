//! Карта нот одной кривой (Note On или Note Off): своя форма у каждой из 128
//! нот. Ключевые ноты (`keys`) — формы с одним и тем же числом узлов N; между
//! соседними ключевыми форма МОРФИТСЯ по узлам: x и y узла i — функции ноты
//! («дорожки морфинга» Lˣᵢ, Lʸᵢ — кубические Безье через значения узла в
//! ключевых нотах), ручки узлов — линейно. Исключения (`overrides`) —
//! одиночные ноты со своей формой (любое число узлов), в морфинге не участвуют.
//!
//! Форма ноты n между соседними ключевыми n_A < n < n_B, узел i:
//!
//!   x_i(n) = Lˣᵢ(n),  y_i(n) = Lʸᵢ(n),
//!   hin_i(n) = (1 − t)·hin_i(A) + t·hin_i(B) (так же `hout`),  t = (n − n_A)/(n_B − n_A).
//!
//! Дорожка на [n_A, n_B] — Безье в координатах (u = нота/127, v = значение
//! узла): P₀ = (n_A/127, v_A), P₁ = P₀ + `hout`(A), P₂ = P₃ + `hin`(B),
//! P₃ = (n_B/127, v_B) ([`NoteMap::lane_seg`]); ручки по u ужаты в ширину
//! отрезка ⇒ дорожка — функция ноты. `auto`-ручки: hout(A) = (P₃ − P₀)/3,
//! hin(B) = (P₀ − P₃)/3 — дорожка прямая, морф линейный. Ниже первой и выше
//! последней ключевой — форма крайней. Изогнутые дорожки x соседних узлов
//! могут пересечься — тогда узлы зажимаются по порядку (`CurveShape::clamp_x`).

use serde::{Deserialize, Serialize};

use super::shape::{add2, bez, fit_handle, lerp2, solve_t, sub2, CurveShape, Knot, P2};

/// Клавиатура в редакторе: 88 клавиш A0 (21) … C8 (108). Ключевые ноты и
/// исключения — только здесь; таблицы — на все 128 нот.
pub const NOTE_LO: u8 = 21;
pub const NOTE_HI: u8 = 108;
/// Нот MIDI (таблиц на кривую).
pub const NOTES: usize = 128;
/// Ключевая нота карты из одной формы (и старого формата файлов).
pub const DEFAULT_KEY: u8 = 60;
/// Нота → u (ось нот дорожки).
const U: f32 = 127.0;

/// Ключевая нота или исключение: форма в ноте `note`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyCurve {
    pub note: u8,
    pub shape: CurveShape,
}

/// Ручки дорожки в одной ключевой ноте: смещения в (u = нота/127, значение).
/// `auto` — ручки вычисляются (прямая к соседней ключевой), пока их не тронули.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LaneKnot {
    pub hin: P2,
    pub hout: P2,
    pub auto: bool,
}

impl LaneKnot {
    pub const AUTO: LaneKnot = LaneKnot { hin: [0.0; 2], hout: [0.0; 2], auto: true };
}

impl Default for LaneKnot {
    fn default() -> Self {
        Self::AUTO
    }
}

/// Ось дорожки: x или y узла.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

impl Axis {
    pub const ALL: [Axis; 2] = [Axis::X, Axis::Y];

    /// Значение узла по этой оси.
    pub fn of(self, k: &Knot) -> f32 {
        match self {
            Axis::X => k.x,
            Axis::Y => k.y,
        }
    }
}

/// Дорожки узла i: значения x и y — из `keys[k].shape.knots[i]` (источник
/// истины один), здесь только ручки — по одной на ключевую ноту.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Lane {
    pub x: Vec<LaneKnot>,
    pub y: Vec<LaneKnot>,
}

impl Lane {
    fn auto(keys: usize) -> Self {
        Self { x: vec![LaneKnot::AUTO; keys], y: vec![LaneKnot::AUTO; keys] }
    }

    pub fn axis(&self, a: Axis) -> &[LaneKnot] {
        match a {
            Axis::X => &self.x,
            Axis::Y => &self.y,
        }
    }

    fn axis_mut(&mut self, a: Axis) -> &mut Vec<LaneKnot> {
        match a {
            Axis::X => &mut self.x,
            Axis::Y => &mut self.y,
        }
    }
}

/// Карта нот одной кривой. Инварианты (держит [`NoteMap::normalize`], его
/// зовёт и чтение из файла): ≥ 1 ключевой, ноты ключевых по возрастанию без
/// повторов в [`NOTE_LO`]…[`NOTE_HI`], у всех ключевых N ≥ 2 узлов,
/// `lanes.len() == N`, у каждой дорожки x и y по `keys.len()` ручек;
/// исключения по возрастанию без повторов.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "NoteMapRepr")]
pub struct NoteMap {
    pub keys: Vec<KeyCurve>,
    pub lanes: Vec<Lane>,
    pub overrides: Vec<KeyCurve>,
}

/// Вид в файле: карта или старая форма `{"knots": [...]}` (до покнотных
/// кривых, формат 2) — она становится картой из одной ключевой ноты.
#[derive(Deserialize)]
#[serde(untagged)]
enum NoteMapRepr {
    Map {
        keys: Vec<KeyCurve>,
        #[serde(default)]
        lanes: Vec<Lane>,
        #[serde(default)]
        overrides: Vec<KeyCurve>,
    },
    Shape(CurveShape),
}

impl From<NoteMapRepr> for NoteMap {
    fn from(r: NoteMapRepr) -> Self {
        let mut m = match r {
            NoteMapRepr::Map { keys, lanes, overrides } => NoteMap { keys, lanes, overrides },
            NoteMapRepr::Shape(s) => NoteMap::single(s),
        };
        m.normalize();
        m
    }
}

impl Default for NoteMap {
    fn default() -> Self {
        Self::single(CurveShape::linear())
    }
}

/// Добавить узлы в самые широкие сегменты (форма не меняется), пока их не
/// станет `n`.
fn pad_knots(shape: &mut CurveShape, n: usize) {
    while shape.knots.len() < n {
        let k = &shape.knots;
        let seg = (0..k.len() - 1).max_by(|&a, &b| (k[a + 1].x - k[a].x).total_cmp(&(k[b + 1].x - k[b].x)));
        shape.split_forced(seg.unwrap_or(0), 0.5);
    }
}

impl NoteMap {
    /// Одна форма на все ноты (ключевая — [`DEFAULT_KEY`]).
    pub fn single(shape: CurveShape) -> Self {
        let n = shape.knots.len();
        Self { keys: vec![KeyCurve { note: DEFAULT_KEY, shape }], lanes: vec![Lane::auto(1); n], overrides: Vec::new() }
    }

    /// Узлов у каждой ключевой.
    pub fn knot_count(&self) -> usize {
        self.keys[0].shape.knots.len()
    }

    /// Индекс ключевой ноты `note`.
    pub fn key_at(&self, note: u8) -> Option<usize> {
        self.keys.binary_search_by_key(&note, |k| k.note).ok()
    }

    /// Индекс исключения на ноте `note`.
    pub fn override_at(&self, note: u8) -> Option<usize> {
        self.overrides.binary_search_by_key(&note, |k| k.note).ok()
    }

    /// Форма ноты `n`: исключение, иначе морф ключевых.
    pub fn shape_at(&self, n: u8) -> CurveShape {
        match self.override_at(n) {
            Some(o) => self.overrides[o].shape.clone(),
            None => self.morph_at(n),
        }
    }

    /// Форма ноты `n` по ключевым (исключения не смотрятся): в ключевой — её
    /// форма бит-в-бит, между ключевыми — морф (см. описание модуля).
    pub fn morph_at(&self, n: u8) -> CurveShape {
        let keys = &self.keys;
        let b = keys.partition_point(|k| k.note < n);
        if b < keys.len() && keys[b].note == n || b == 0 {
            return keys[b].shape.clone();
        }
        if b == keys.len() {
            return keys[b - 1].shape.clone();
        }
        let a = b - 1;
        let (ka, kb) = (&keys[a], &keys[b]);
        let t = (n - ka.note) as f32 / (kb.note - ka.note) as f32;
        let u = n as f32 / U;
        let at = |axis, i| {
            let p = self.lane_seg(axis, i, a);
            bez(&p, solve_t(&p, u, 0.0))[1]
        };
        let knots = ka
            .shape
            .knots
            .iter()
            .zip(&kb.shape.knots)
            .enumerate()
            .map(|(i, (qa, qb))| Knot {
                x: at(Axis::X, i),
                y: at(Axis::Y, i),
                hin: lerp2(qa.hin, qb.hin, t),
                hout: lerp2(qa.hout, qb.hout, t),
                smooth: if t < 0.5 { qa.smooth } else { qb.smooth },
            })
            .collect();
        let mut s = CurveShape { knots };
        s.clamp_x();
        s
    }

    /// Узел дорожки `axis` узла `i` в ключевой `k`: (нота/127, значение).
    pub fn lane_pt(&self, axis: Axis, i: usize, k: usize) -> P2 {
        let key = &self.keys[k];
        [key.note as f32 / U, axis.of(&key.shape.knots[i])]
    }

    /// Действующая входящая ручка дорожки в ключевой `k` (как хранится, без
    /// ужатия; `auto` — треть хорды к предыдущей ключевой).
    pub fn lane_hin(&self, axis: Axis, i: usize, k: usize) -> P2 {
        let lk = self.lanes[i].axis(axis)[k];
        match (lk.auto, k) {
            (false, _) => lk.hin,
            (true, 0) => [0.0; 2],
            (true, _) => {
                let d = sub2(self.lane_pt(axis, i, k - 1), self.lane_pt(axis, i, k));
                [d[0] / 3.0, d[1] / 3.0]
            }
        }
    }

    /// Действующая исходящая ручка дорожки в ключевой `k`.
    pub fn lane_hout(&self, axis: Axis, i: usize, k: usize) -> P2 {
        let lk = self.lanes[i].axis(axis)[k];
        if !lk.auto {
            return lk.hout;
        }
        if k + 1 >= self.keys.len() {
            return [0.0; 2];
        }
        let d = sub2(self.lane_pt(axis, i, k + 1), self.lane_pt(axis, i, k));
        [d[0] / 3.0, d[1] / 3.0]
    }

    /// Контрольные точки дорожки между ключевыми `a` и `a+1` (ручки ужаты в
    /// ширину отрезка по оси нот).
    pub fn lane_seg(&self, axis: Axis, i: usize, a: usize) -> [P2; 4] {
        let (p0, p3) = (self.lane_pt(axis, i, a), self.lane_pt(axis, i, a + 1));
        let w = p3[0] - p0[0];
        let ho = fit_handle(self.lane_hout(axis, i, a), w, 1.0);
        let hi = fit_handle(self.lane_hin(axis, i, a + 1), w, -1.0);
        [p0, add2(p0, ho), add2(p3, hi), p3]
    }

    /// Значение дорожки в ноте `n` (дробной) — до зажима порядка узлов.
    pub fn lane_value(&self, axis: Axis, i: usize, n: f32) -> f32 {
        let keys = &self.keys;
        let b = keys.partition_point(|k| (k.note as f32) < n);
        if b == 0 {
            return axis.of(&keys[0].shape.knots[i]);
        }
        if b == keys.len() {
            return axis.of(&keys[b - 1].shape.knots[i]);
        }
        if keys[b].note as f32 == n {
            return axis.of(&keys[b].shape.knots[i]);
        }
        let p = self.lane_seg(axis, i, b - 1);
        bez(&p, solve_t(&p, n / U, 0.0))[1]
    }

    /// Ноты, чья форма зависит от ключевой `k` (от соседней слева до соседней
    /// справа, у крайней — до края), включительно.
    pub fn key_span(&self, k: usize) -> (u8, u8) {
        let lo = if k == 0 { 0 } else { self.keys[k - 1].note };
        let hi = if k + 1 >= self.keys.len() { (NOTES - 1) as u8 } else { self.keys[k + 1].note };
        (lo, hi)
    }

    /// Сделать ноту `n` ключевой, НЕ меняя ни одной ноты: форма = морф в `n`,
    /// каждая дорожка делится в `n` (де Кастельжо). Ниже первой / выше
    /// последней — форма крайней и ровная дорожка до неё. Индекс ключевой.
    pub fn add_key(&mut self, n: u8) -> usize {
        let n = n.clamp(NOTE_LO, NOTE_HI);
        let b = self.keys.partition_point(|k| k.note < n);
        if b < self.keys.len() && self.keys[b].note == n {
            return b;
        }
        let shape = self.morph_at(n);
        let (nk, len) = (self.knot_count(), self.keys.len());
        let mut new = vec![[LaneKnot::AUTO; 2]; nk];
        if b == 0 || b == len {
            // Снаружи: отрезок до крайней ключевой — ровный (значения равны),
            // ручку крайней в его сторону кладём горизонтально.
            let e = if b == 0 { 0 } else { len - 1 };
            let w = (n as f32 - self.keys[e].note as f32).abs() / U;
            for lane in &mut self.lanes {
                for axis in Axis::ALL {
                    let lk = &mut lane.axis_mut(axis)[e];
                    if !lk.auto {
                        if b == 0 {
                            lk.hin = [-w / 3.0, 0.0];
                        } else {
                            lk.hout = [w / 3.0, 0.0];
                        }
                    }
                }
            }
        } else {
            let a = b - 1;
            let u = n as f32 / U;
            for (i, slot) in new.iter_mut().enumerate() {
                for (j, axis) in Axis::ALL.into_iter().enumerate() {
                    let (la, lb) = (self.lanes[i].axis(axis)[a], self.lanes[i].axis(axis)[b]);
                    if la.auto && lb.auto {
                        // Прямая делится на прямые: все ручки остаются auto.
                        continue;
                    }
                    let p = self.lane_seg(axis, i, a);
                    let t = solve_t(&p, u, 0.0);
                    let (p01, p12, p23) = (lerp2(p[0], p[1], t), lerp2(p[1], p[2], t), lerp2(p[2], p[3], t));
                    let (p012, p123) = (lerp2(p01, p12, t), lerp2(p12, p23, t));
                    let m = lerp2(p012, p123, t);
                    let (hin_a, hout_b) = (self.lane_hin(axis, i, a), self.lane_hout(axis, i, b));
                    let v = self.lanes[i].axis_mut(axis);
                    v[a] = LaneKnot { hin: hin_a, hout: sub2(p01, p[0]), auto: false };
                    v[b] = LaneKnot { hin: sub2(p23, p[3]), hout: hout_b, auto: false };
                    slot[j] = LaneKnot { hin: sub2(p012, m), hout: sub2(p123, m), auto: false };
                }
            }
        }
        self.keys.insert(b, KeyCurve { note: n, shape });
        for (lane, [x, y]) in self.lanes.iter_mut().zip(new) {
            lane.x.insert(b, x);
            lane.y.insert(b, y);
        }
        b
    }

    /// Убрать ключевую `k` (последнюю не убрать). `true` — убрана.
    pub fn remove_key(&mut self, k: usize) -> bool {
        if self.keys.len() <= 1 || k >= self.keys.len() {
            return false;
        }
        self.keys.remove(k);
        for lane in &mut self.lanes {
            lane.x.remove(k);
            lane.y.remove(k);
        }
        true
    }

    /// Перенести ключевую `k` на ноту `n` — между соседними ключевыми и в
    /// пределах клавиатуры. Ручки дорожек — смещения, ужимаются при
    /// вычислении. Нота, куда встала.
    pub fn move_key(&mut self, k: usize, n: u8) -> u8 {
        let lo = if k == 0 { NOTE_LO } else { self.keys[k - 1].note + 1 };
        let hi = if k + 1 >= self.keys.len() { NOTE_HI } else { self.keys[k + 1].note - 1 };
        let n = n.clamp(lo, hi.max(lo));
        self.keys[k].note = n;
        n
    }

    /// Вставить узел во ВСЕ ключевые ноты в сегмент `seg` при параметре `t`
    /// (формы ключевых не меняются); новая дорожка — `auto`. Индекс узла.
    pub fn insert_knot(&mut self, seg: usize, t: f32) -> Option<usize> {
        if seg + 1 >= self.knot_count() {
            return None;
        }
        for key in &mut self.keys {
            key.shape.split_forced(seg, t);
        }
        self.lanes.insert(seg + 1, Lane::auto(self.keys.len()));
        Some(seg + 1)
    }

    /// Удалить внутренний узел `i` из всех ключевых нот вместе с дорожкой.
    pub fn remove_knot(&mut self, i: usize) -> bool {
        if i == 0 || i + 1 >= self.knot_count() {
            return false;
        }
        for key in &mut self.keys {
            key.shape.knots.remove(i);
        }
        self.lanes.remove(i);
        true
    }

    /// Поставить исключение на ноту `n` (своя форма, вне морфинга).
    pub fn set_override(&mut self, n: u8, shape: CurveShape) {
        let n = n.clamp(NOTE_LO, NOTE_HI);
        match self.overrides.binary_search_by_key(&n, |k| k.note) {
            Ok(o) => self.overrides[o].shape = shape,
            Err(o) => self.overrides.insert(o, KeyCurve { note: n, shape }),
        }
    }

    pub fn remove_override(&mut self, n: u8) -> bool {
        match self.override_at(n) {
            Some(o) => {
                self.overrides.remove(o);
                true
            }
            None => false,
        }
    }

    /// Привести форму к числу узлов карты: у формы меньше — узлы добавляются в
    /// самые широкие её сегменты; больше — в карту (во все ключевые). Ни одна
    /// форма при этом не меняется (кроме морфа между ключевыми из-за новых
    /// узлов карты). Для загрузки формы из библиотеки в ключевую ноту.
    pub fn fit_shape(&mut self, mut shape: CurveShape) -> CurveShape {
        while self.knot_count() < shape.knots.len() {
            let n = self.knot_count();
            let width = |s: usize| self.keys.iter().map(|k| k.shape.knots[s + 1].x - k.shape.knots[s].x).sum::<f32>();
            let seg = (0..n - 1).max_by(|&a, &b| width(a).total_cmp(&width(b))).unwrap_or(0);
            self.insert_knot(seg, 0.5);
        }
        pad_knots(&mut shape, self.knot_count());
        shape
    }

    /// Ручка дорожки `axis` узла `i` в ключевой `k` — смещение `v` от узла
    /// дорожки (нота/127, значение). Снимает `auto` (другая ручка остаётся
    /// прежней действующей); `sym` — другая ручка зеркально через узел (ЛКМ),
    /// ход ужат в ширину УЖЕГО из соседних отрезков. Ручки наружу крайних
    /// ключевых нет.
    pub fn set_lane_handle(&mut self, axis: Axis, i: usize, k: usize, out: bool, v: P2, sym: bool) {
        let len = self.keys.len();
        let has = |out: bool| if out { k + 1 < len } else { k > 0 };
        if !has(out) {
            return;
        }
        let width = |out: bool| {
            if !has(out) {
                return 0.0;
            }
            let j = if out { k + 1 } else { k - 1 };
            (self.keys[j].note as f32 - self.keys[k].note as f32).abs() / U
        };
        let sign = if out { 1.0 } else { -1.0 };
        let (w_own, w_other) = (width(out), width(!out));
        let (hin, hout) = (self.lane_hin(axis, i, k), self.lane_hout(axis, i, k));
        let mirror = sym && has(!out);
        let v = fit_handle(v, if mirror { w_own.min(w_other) } else { w_own }, sign);
        let (mut hin, mut hout) = (hin, hout);
        if out {
            hout = v;
        } else {
            hin = v;
        }
        if mirror {
            let m = [-v[0], -v[1]];
            if out {
                hin = m;
            } else {
                hout = m;
            }
        }
        self.lanes[i].axis_mut(axis)[k] = LaneKnot { hin, hout, auto: false };
    }

    /// Вернуть ручкам дорожки в ключевой `k` автоматический (линейный) морф.
    pub fn set_lane_auto(&mut self, axis: Axis, i: usize, k: usize) {
        self.lanes[i].axis_mut(axis)[k] = LaneKnot::AUTO;
    }

    /// Привести карту в допустимую (см. инварианты [`NoteMap`]): чинит
    /// карты из файлов и проектов. Если ключевые пришлось переставить,
    /// сдвинуть или выровнять по числу узлов — дорожки сбрасываются в `auto`.
    pub fn normalize(&mut self) {
        if self.keys.is_empty() {
            *self = Self::default();
            return;
        }
        let mut keys_ok = true;
        for key in &mut self.keys {
            let n = key.note.clamp(NOTE_LO, NOTE_HI);
            keys_ok &= n == key.note;
            key.note = n;
            key.shape.normalize();
        }
        if !self.keys.windows(2).all(|w| w[0].note < w[1].note) {
            keys_ok = false;
            self.keys.sort_by_key(|k| k.note);
            self.keys.dedup_by_key(|k| k.note);
        }
        let n = self.keys.iter().map(|k| k.shape.knots.len()).max().unwrap_or(2);
        for key in &mut self.keys {
            if key.shape.knots.len() < n {
                keys_ok = false;
                pad_knots(&mut key.shape, n);
            }
        }
        let len = self.keys.len();
        let lanes_ok = keys_ok
            && self.lanes.len() == n
            && self.lanes.iter().all(|l| l.x.len() == len && l.y.len() == len);
        if !lanes_ok {
            self.lanes = vec![Lane::auto(len); n];
        }
        for lane in &mut self.lanes {
            for lk in lane.x.iter_mut().chain(lane.y.iter_mut()) {
                if !lk.hin.iter().chain(&lk.hout).all(|v| v.is_finite()) {
                    *lk = LaneKnot::AUTO;
                }
            }
        }
        for o in &mut self.overrides {
            o.note = o.note.clamp(NOTE_LO, NOTE_HI);
            o.shape.normalize();
        }
        self.overrides.sort_by_key(|o| o.note);
        self.overrides.dedup_by_key(|o| o.note);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::curve::shape::KNOT_GAP;

    /// Детерминированный генератор (xorshift64*) для случайных карт.
    pub(crate) struct Rng(u64);

    impl Rng {
        pub(crate) fn new(seed: u64) -> Self {
            Self(seed.max(1))
        }

        pub(crate) fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        /// Равномерно в [lo, hi).
        pub(crate) fn f(&mut self, lo: f32, hi: f32) -> f32 {
            lo + (hi - lo) * ((self.next() >> 40) as f32 / (1u64 << 24) as f32)
        }

        pub(crate) fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    /// Случайная форма из `n` узлов: x по возрастанию с зазором, ход ручек по
    /// x — до доли `hx` ширины своего сегмента (больше 1 — за сегмент,
    /// ужимается). При `hx` ≲ 0.45 деление сегмента де Кастельжо точно:
    /// ручки кусков не выходят за их ширину.
    pub(crate) fn random_shape(r: &mut Rng, n: usize, hx: f32) -> CurveShape {
        let mut xs: Vec<f32> = (0..n - 2).map(|_| r.f(0.02, 0.98)).collect();
        xs.sort_by(f32::total_cmp);
        let mut s = CurveShape::linear();
        for x in xs {
            let _ = s.insert_at(x);
        }
        while s.knots.len() < n {
            let x = r.f(0.05, 0.95);
            let _ = s.insert_at(x);
        }
        for i in 0..n {
            let x = s.knots[i].x;
            s.move_knot(i, x, r.f(0.0, 1.0));
            let w_in = if i > 0 { x - s.knots[i - 1].x } else { 0.0 };
            let w_out = if i + 1 < n { s.knots[i + 1].x - x } else { 0.0 };
            s.knots[i].hin = [-w_in * r.f(0.1, hx), r.f(-0.3, 0.3)];
            s.knots[i].hout = [w_out * r.f(0.1, hx), r.f(-0.3, 0.3)];
        }
        s
    }

    /// Случайная карта: `keys` ключевых по `n` узлов, ручки дорожек — изогнутые
    /// (ход по оси нот в доле `bend` от ширины отрезка), часть — `auto`.
    pub(crate) fn random_map(r: &mut Rng, keys: usize, n: usize, bend: (f32, f32), hx: f32) -> NoteMap {
        let mut notes: Vec<u8> = Vec::new();
        while notes.len() < keys {
            let note = NOTE_LO + r.below((NOTE_HI - NOTE_LO + 1) as usize) as u8;
            if !notes.contains(&note) {
                notes.push(note);
            }
        }
        notes.sort();
        let mut m = NoteMap {
            keys: notes.iter().map(|&note| KeyCurve { note, shape: random_shape(r, n, hx) }).collect(),
            lanes: vec![Lane::auto(keys); n],
            overrides: Vec::new(),
        };
        for i in 0..n {
            for axis in Axis::ALL {
                for k in 0..keys {
                    if r.below(3) == 0 {
                        continue;
                    }
                    let w = |j: usize| (m.keys[j].note as f32 - m.keys[k].note as f32).abs() / U;
                    let wi = if k > 0 { w(k - 1) } else { 0.0 };
                    let wo = if k + 1 < keys { w(k + 1) } else { 0.0 };
                    let lk = &mut m.lanes[i].axis_mut(axis)[k];
                    lk.auto = false;
                    lk.hin = [-wi * r.f(bend.0, bend.1), r.f(-0.3, 0.3)];
                    lk.hout = [wo * r.f(bend.0, bend.1), r.f(-0.3, 0.3)];
                }
            }
        }
        m
    }

    /// Наибольшее расхождение форм двух карт по всем нотам: (по параметрам
    /// узлов — x, y, ручки; по таблице 257 точек; нота худшего).
    pub(crate) fn max_diff(a: &NoteMap, b: &NoteMap) -> (f32, f32, u8) {
        let mut worst = (0.0f32, 0.0f32, 0u8);
        for n in 0..NOTES as u8 {
            let (sa, sb) = (a.shape_at(n), b.shape_at(n));
            assert_eq!(sa.knots.len(), sb.knots.len(), "нота {n}");
            for (ka, kb) in sa.knots.iter().zip(&sb.knots) {
                let d = [ka.x - kb.x, ka.y - kb.y, ka.hin[0] - kb.hin[0], ka.hin[1] - kb.hin[1]]
                    .into_iter()
                    .chain([ka.hout[0] - kb.hout[0], ka.hout[1] - kb.hout[1]])
                    .fold(0.0f32, |m, v| m.max(v.abs()));
                if d > worst.0 {
                    worst = (d, worst.1, n);
                }
            }
            let (mut la, mut lb) = ([0.0f32; 257], [0.0f32; 257]);
            sa.fill_lut(&mut la);
            sb.fill_lut(&mut lb);
            worst.1 = la.iter().zip(&lb).fold(worst.1, |m, (ya, yb)| m.max((ya - yb).abs()));
        }
        worst
    }

    fn two_keys() -> NoteMap {
        let mut r = Rng::new(7);
        let mut m = NoteMap::single(random_shape(&mut r, 4, 0.45));
        m.keys[0].note = 30;
        m.add_key(90);
        m.keys[1].shape = random_shape(&mut r, 4, 0.45);
        m.normalize();
        m
    }

    #[test]
    fn single_key_is_the_shape_everywhere_bit_exact() {
        let mut r = Rng::new(1);
        let s = random_shape(&mut r, 5, 0.45);
        let m = NoteMap::single(s.clone());
        for n in 0..NOTES as u8 {
            assert_eq!(m.shape_at(n), s, "нота {n}");
        }
    }

    #[test]
    fn auto_lanes_morph_linearly() {
        let m = two_keys();
        let (a, b) = (&m.keys[0], &m.keys[1]);
        for n in 0..NOTES as u8 {
            let s = m.shape_at(n);
            let t = ((n as f32 - 30.0) / 60.0).clamp(0.0, 1.0);
            for (i, k) in s.knots.iter().enumerate() {
                let (qa, qb) = (a.shape.knots[i], b.shape.knots[i]);
                let lin = |va: f32, vb: f32| va + (vb - va) * t;
                let e = [
                    k.x - lin(qa.x, qb.x),
                    k.y - lin(qa.y, qb.y),
                    k.hin[0] - lin(qa.hin[0], qb.hin[0]),
                    k.hin[1] - lin(qa.hin[1], qb.hin[1]),
                    k.hout[0] - lin(qa.hout[0], qb.hout[0]),
                    k.hout[1] - lin(qa.hout[1], qb.hout[1]),
                ];
                assert!(e.iter().all(|d| d.abs() < 1e-5), "нота {n}, узел {i}: {e:?}");
            }
        }
    }

    /// Есть ли нота, где дорожки x пересеклись и узел зажат (§1.3 плана):
    /// там вставка ключевой берёт зажатое значение и точной быть не может.
    fn has_clamped_x(m: &NoteMap) -> bool {
        (0..NOTES as u8).any(|n| {
            let s = m.morph_at(n);
            (1..s.knots.len() - 1).any(|i| (m.lane_value(Axis::X, i, n as f32) - s.knots[i].x).abs() > 1e-7)
        })
    }

    #[test]
    fn add_key_changes_no_note() {
        let mut tested = 0;
        for seed in 0..60 {
            let mut r = Rng::new(100 + seed);
            let (keys, knots) = (1 + r.below(4), 2 + r.below(4));
            let mut m = random_map(&mut r, keys, knots, (0.15, 0.45), 0.45);
            m.normalize();
            if has_clamped_x(&m) {
                continue;
            }
            tested += 1;
            for _ in 0..6 {
                let before = m.clone();
                let n = NOTE_LO + r.below((NOTE_HI - NOTE_LO + 1) as usize) as u8;
                let k = m.add_key(n);
                assert_eq!(m.keys[k].note, n);
                // Узлы — до округления f32; таблица — с запасом на крутизну
                // случайных форм (1 ulp узла × наклон до ~100).
                let (d, dy, at) = max_diff(&before, &m);
                assert!(d < 1e-5 && dy < 1e-4, "seed {seed}: ключевая {n} сдвинула ноту {at}: {d} / {dy}");
            }
            // Ключевая, которая уже есть, — без изменений.
            let before = m.clone();
            let n = m.keys[0].note;
            assert_eq!(m.add_key(n), 0);
            assert_eq!(m, before);
        }
        assert!(tested >= 30, "проверено карт: {tested}");
    }

    #[test]
    fn insert_knot_keeps_key_shapes() {
        let mut r = Rng::new(5);
        let mut m = random_map(&mut r, 3, 4, (0.1, 0.5), 0.45);
        m.normalize();
        let before = m.clone();
        let i = m.insert_knot(1, 0.4).unwrap();
        assert_eq!(i, 2);
        assert_eq!(m.knot_count(), 5);
        assert_eq!(m.lanes.len(), 5);
        assert!(m.lanes[i].x.iter().chain(&m.lanes[i].y).all(|l| l.auto));
        for (k0, k1) in before.keys.iter().zip(&m.keys) {
            for j in 0..=200 {
                let x = j as f32 / 200.0;
                assert!((k0.shape.eval(x) - k1.shape.eval(x)).abs() < 1e-5, "нота {}, x={x}", k0.note);
            }
        }
        assert!(m.remove_knot(i));
        assert_eq!(m.knot_count(), 4);
        assert!(!m.remove_knot(0) && !m.remove_knot(3));
        assert!(m.insert_knot(3, 0.5).is_none());
    }

    #[test]
    fn bent_lanes_keep_every_note_valid() {
        let mut r = Rng::new(42);
        let maps = if cfg!(debug_assertions) { 200 } else { 1000 };
        for _ in 0..maps {
            let (keys, knots) = (1 + r.below(5), 2 + r.below(6));
            // Ход ручек по нотам и за отрезок (ужимается), дорожки пересекаются.
            let mut m = random_map(&mut r, keys, knots, (0.0, 1.5), 3.0);
            m.normalize();
            for n in 0..NOTES as u8 {
                let s = m.shape_at(n);
                let k = &s.knots;
                assert_eq!((k[0].x, k[k.len() - 1].x), (0.0, 1.0));
                for w in k.windows(2) {
                    assert!(w[1].x - w[0].x >= KNOT_GAP * 0.999, "нота {n}: узлы не по порядку {k:?}");
                }
                // Выход зажат в [0, 1]; NaN зажим не ловит — проверка на конечность.
                let mut lut = [0.0f32; 65];
                s.fill_lut(&mut lut);
                assert!(lut.iter().all(|y| (0.0..=1.0).contains(y)), "нота {n}: выход {lut:?}");
            }
        }
    }

    #[test]
    fn override_is_only_its_note() {
        let mut r = Rng::new(9);
        let mut m = random_map(&mut r, 3, 4, (0.1, 0.4), 0.45);
        m.normalize();
        let plain = m.clone();
        let own = random_shape(&mut r, 7, 0.45);
        m.set_override(50, own.clone());
        for n in 0..NOTES as u8 {
            if n == 50 {
                assert_eq!(m.shape_at(n), own);
            } else {
                assert_eq!(m.shape_at(n), plain.shape_at(n), "нота {n}");
            }
        }
        assert!(m.remove_override(50) && !m.remove_override(50));
        assert_eq!(m, plain);
    }

    #[test]
    fn keys_move_and_remove_between_neighbours() {
        let mut m = two_keys();
        let k = m.add_key(60);
        assert_eq!(m.move_key(k, 20), 31);
        assert_eq!(m.move_key(k, 100), 89);
        assert_eq!(m.move_key(0, 0), NOTE_LO);
        assert_eq!(m.move_key(2, 127), NOTE_HI);
        assert_eq!(m.key_span(0), (0, 89));
        assert_eq!(m.key_span(1), (NOTE_LO, NOTE_HI));
        assert_eq!(m.key_span(2), (89, 127));
        assert!(m.remove_key(1));
        assert!(m.remove_key(0));
        assert!(!m.remove_key(0), "последнюю ключевую не убрать");
        assert_eq!(m.lanes[0].x.len(), 1);
    }

    #[test]
    fn fit_shape_equalizes_knot_counts() {
        let mut r = Rng::new(3);
        let mut m = random_map(&mut r, 2, 4, (0.1, 0.4), 0.45);
        m.normalize();
        // Меньше узлов — дополняется, форма та же.
        let small = CurveShape::linear();
        let fitted = m.fit_shape(small.clone());
        assert_eq!(fitted.knots.len(), 4);
        for j in 0..=100 {
            let x = j as f32 / 100.0;
            assert!((fitted.eval(x) - small.eval(x)).abs() < 1e-5);
        }
        // Больше — узлы добавляются в карту, формы ключевых те же.
        let before = m.clone();
        let big = random_shape(&mut r, 7, 0.45);
        let fitted = m.fit_shape(big.clone());
        assert_eq!(fitted, big);
        assert_eq!(m.knot_count(), 7);
        for (k0, k1) in before.keys.iter().zip(&m.keys) {
            for j in 0..=100 {
                let x = j as f32 / 100.0;
                assert!((k0.shape.eval(x) - k1.shape.eval(x)).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn lane_handles_auto_and_symmetric() {
        let mut m = two_keys();
        let k = m.add_key(50);
        let lin = m.clone();
        // ЛКМ: вторая ручка зеркальна, ход ужат в узкий отрезок (20 нот слева).
        m.set_lane_handle(Axis::Y, 1, k, true, [0.5, 0.2], true);
        let (hi, ho) = (m.lane_hin(Axis::Y, 1, k), m.lane_hout(Axis::Y, 1, k));
        assert!((ho[0] - 20.0 / U).abs() < 1e-6 && (hi[0] + ho[0]).abs() < 1e-7 && (hi[1] + ho[1]).abs() < 1e-7);
        assert!(!m.lanes[1].y[k].auto);
        // Меняются только ноты отрезков у этой ключевой.
        for n in 0..NOTES as u8 {
            let same = m.shape_at(n) == lin.shape_at(n);
            assert_eq!(same, !(31..90).contains(&n) || n == 50, "нота {n}");
        }
        // ПКМ: другая ручка прежняя.
        let before_in = m.lane_hin(Axis::Y, 1, k);
        m.set_lane_handle(Axis::Y, 1, k, true, [0.05, -0.1], false);
        assert_eq!(m.lane_hin(Axis::Y, 1, k), before_in);
        // auto — снова линейный морф, карта бит-в-бит прежняя.
        m.set_lane_auto(Axis::Y, 1, k);
        assert_eq!(m, lin);
    }

    #[test]
    fn normalize_repairs_broken_maps() {
        let mut r = Rng::new(11);
        let mut m = random_map(&mut r, 3, 4, (0.1, 0.4), 0.45);
        m.keys.swap(0, 2);
        m.keys[1].note = 3;
        let dup = m.keys[0].clone();
        m.keys.push(dup);
        m.keys[2].shape = CurveShape::linear();
        m.lanes.pop();
        m.lanes[0].x[0].hout[0] = f32::NAN;
        m.overrides = vec![
            KeyCurve { note: 200, shape: CurveShape::linear() },
            KeyCurve { note: 40, shape: CurveShape { knots: vec![] } },
        ];
        m.normalize();
        assert!(m.keys.windows(2).all(|w| w[0].note < w[1].note));
        assert!(m.keys.iter().all(|k| (NOTE_LO..=NOTE_HI).contains(&k.note)));
        let n = m.knot_count();
        assert!(m.keys.iter().all(|k| k.shape.knots.len() == n));
        assert_eq!(m.lanes.len(), n);
        assert!(m.lanes.iter().all(|l| l.x.len() == m.keys.len() && l.y.len() == m.keys.len()));
        assert_eq!(m.overrides.iter().map(|o| o.note).collect::<Vec<_>>(), vec![40, NOTE_HI]);
        assert_eq!(m.overrides[0].shape, CurveShape::linear());
        for note in 0..NOTES as u8 {
            m.shape_at(note);
        }
        let mut empty = NoteMap { keys: vec![], lanes: vec![], overrides: vec![] };
        empty.normalize();
        assert_eq!(empty, NoteMap::default());
    }

    #[test]
    fn serde_reads_old_shape_and_roundtrips_map() {
        let mut r = Rng::new(12);
        let s = random_shape(&mut r, 3, 0.45);
        let old = serde_json::to_string(&s).unwrap();
        let m: NoteMap = serde_json::from_str(&old).unwrap();
        assert_eq!(m, NoteMap::single(s));
        let mut m = random_map(&mut r, 4, 5, (0.1, 0.6), 0.45);
        m.normalize();
        m.set_override(70, random_shape(&mut r, 3, 0.45));
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(serde_json::from_str::<NoteMap>(&json).unwrap(), m);
        // Битая карта из файла чинится при чтении.
        let broken = r#"{"keys": [{"note": 90, "shape": {"knots": []}}, {"note": 30, "shape": {"knots": []}}]}"#;
        let m: NoteMap = serde_json::from_str(broken).unwrap();
        assert_eq!(m.keys.iter().map(|k| k.note).collect::<Vec<_>>(), vec![30, 90]);
        assert_eq!(m.lanes.len(), 2);
    }
}

