//! Редактор кривых velocity (egui) — общий для плагина и standalone. Жесты и
//! отрисовка графика — из редактора piano_sim (`src/gui/vel_curve_editor.rs`).
//!
//! У каждой кривой (Note On / Note Off) — карта нот ([`NoteMap`]): клавиатура
//! внизу выбирает ноту, график показывает и правит её форму. Ключевая нота и
//! исключение правятся как есть; первая правка промежуточной ноты делает её
//! ключевой (вставка точная — ни одна нота от этого не меняется). Вставка и
//! удаление узла на ключевой — сразу во всех ключевых. Соседние ключевые —
//! бледными призраками на графике.
//!
//! Мышь на графике: ЛКМ по кривой — новый узел (форма не меняется), драг узла
//! или ручки — изгиб (ручка — симметрично: вторая зеркально через узел),
//! ПКМ-драг ручки — ручки по отдельности, ПКМ или двойной клик по узлу —
//! удалить его, Delete — удалить выбранный. Shift — точный ход (×0.1), Ctrl — привязка к сетке 1/16,
//! Alt+драг ручки — сломать узел в угол, Alt+клик по узлу — гладкий ↔ угол.
//! Клавиатура: клик — выбрать ноту, драг ключевой — перенести её между
//! соседними ключевыми, ПКМ — меню (ключевая / исключение / убрать).
//! Дорожка морфинга (над клавиатурой) — как узел, выбранный на графике, идёт
//! по клавиатуре: узлы дорожки — ключевые ноты (драг по вертикали — значение
//! узла в этой ключевой), ручки выбранной ключевой гнут морф (ЛКМ —
//! симметрично, ПКМ — по отдельности), двойной клик по узлу — снова линейно.
//! Режим x — веер дорожек x всех узлов, зажатые места (пересечение) — красным.
//! Ctrl+Z / Ctrl+Shift+Z (Ctrl+Y) — отмена / повтор, история у каждой кривой
//! своя (вся карта нот); снимок — на отпускании мыши.
//!
//! Экономия CPU: ломаная кривой (257 точек) пересчитывается только при
//! изменении формы, призраки берутся из таблиц банка; таблицы потока MIDI
//! перезаливает вызывающий — только ноты из [`Outcome::dirty`].

use crate::curve::notemap::{DEFAULT_KEY, NOTES, NOTE_HI, NOTE_LO};
use crate::curve::shape::P2;
use crate::curve::{Axis, CurveBank, CurveSet, CurveShape, NoteMap, Slot};
use crate::egui::{self, Color32, Pos2, Rect, Stroke};
use crate::library::{self, Entry, Library, Preset};

/// Радиусы захвата узла и ручки, пиксели.
const HIT_KNOT: f32 = 8.0;
const HIT_HANDLE: f32 = 7.0;
/// Шаг привязки (Ctrl) в единичном квадрате.
const SNAP: f32 = 1.0 / 16.0;
/// Отрезков ломаной кривой на графике.
const POLY_SEGS: usize = 256;
/// Отрезков ломаной призрака (соседней ключевой).
const GHOST_SEGS: usize = 128;
/// Глубина истории отмены каждой кривой.
const HISTORY_DEPTH: usize = 100;
/// Поля области рисования слева и справа (график и клавиатура — одинаково).
const MARGIN_L: f32 = 40.0;
const MARGIN_R: f32 = 12.0;
/// Высота клавиш и подписей под ними.
const KB_H: f32 = 34.0;
const KB_LABEL_H: f32 = 14.0;
/// Высота поля дорожки морфинга.
const LANE_H: f32 = 110.0;
/// Точек на отрезок дорожки при отрисовке.
const LANE_SEGS: usize = 16;
/// Клавиш на клавиатуре редактора (A0…C8).
const KEYS: usize = (NOTE_HI - NOTE_LO + 1) as usize;
const LAST_NOTE: u8 = (NOTES - 1) as u8;

const COL_NOTE_ON: Color32 = Color32::from_rgb(100, 200, 255);
const COL_NOTE_OFF: Color32 = Color32::from_rgb(255, 150, 100);
const COL_MARKER: Color32 = Color32::from_rgb(120, 230, 140);
const COL_OVERRIDE: Color32 = Color32::from_rgb(235, 195, 80);
const COL_SET: Color32 = Color32::from_rgb(220, 200, 140);
const COL_CLAMP: Color32 = Color32::from_rgb(240, 80, 70);

fn color(slot: Slot) -> Color32 {
    match slot {
        Slot::NoteOn => COL_NOTE_ON,
        Slot::NoteOff => COL_NOTE_OFF,
    }
}

fn slot_name(slot: Slot) -> &'static str {
    match slot {
        Slot::NoteOn => "Note On",
        Slot::NoteOff => "Note Off",
    }
}

/// Имя ноты MIDI: 60 — C4, 21 — A0.
pub fn note_name(n: u8) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", NAMES[(n % 12) as usize], (n / 12) as i32 - 1)
}

fn note_label(n: u8) -> String {
    format!("{} ({n})", note_name(n))
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Grab {
    Knot(usize),
    In(usize),
    Out(usize),
}

/// Чем нота является в карте.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Role {
    /// Исключение (индекс в `overrides`).
    Override(usize),
    /// Ключевая (индекс в `keys`).
    Key(usize),
    /// Промежуточная (или за крайней ключевой): форма — морф.
    Morph,
}

fn role(map: &NoteMap, n: u8) -> Role {
    match (map.override_at(n), map.key_at(n)) {
        (Some(o), _) => Role::Override(o),
        (None, Some(k)) => Role::Key(k),
        (None, None) => Role::Morph,
    }
}

/// Ближайшие ключевые ноты слева и справа от `n` (строго).
fn neighbours(map: &NoteMap, n: u8) -> (Option<u8>, Option<u8>) {
    let lo = map.keys.iter().rev().map(|k| k.note).find(|&k| k < n);
    let hi = map.keys.iter().map(|k| k.note).find(|&k| k > n);
    (lo, hi)
}

/// Смена числа узлов формы на графике — её надо повторить во всей карте.
#[derive(Clone, Copy, PartialEq, Debug)]
enum KnotOp {
    /// Узел вставлен в сегмент при параметре t.
    Insert(usize, f32),
    Remove(usize),
}

/// Что сделал жест на графике за кадр.
#[derive(Default)]
struct Edit {
    changed: bool,
    ops: Vec<KnotOp>,
}

/// Вид и жест одной кривой.
struct CurveView {
    /// Выбранная нота: её форма на графике.
    note: u8,
    /// Выбранный узел (его ручки видны и хватаются).
    sel: Option<usize>,
    grab: Option<Grab>,
    /// Позиция захваченного объекта БЕЗ клампа: шаг мыши копится сюда, чтобы
    /// упор в край или соседний узел не «прилипал» при обратном ходе.
    grab_pos: [f32; 2],
    /// Текущий драг ручки — симметричный (ЛКМ; ПКМ — ручки по отдельности).
    sym: bool,
    /// Ломаная в единичном квадрате и форма, по которой она посчитана.
    poly: Vec<[f32; 2]>,
    poly_of: Option<CurveShape>,
    /// Драг ключевой на клавиатуре: индекс ключевой.
    key_drag: Option<usize>,
    /// Нота под меню ПКМ клавиатуры.
    menu_note: u8,
    /// Последний клик по узлу графика: (узел, время). Двойной клик удаляет
    /// узел, только если и первый клик был по нему: egui считает двойным и
    /// клик по клавише, за которым сразу клик по графику.
    knot_click: Option<(usize, f64)>,
    /// Жест на дорожке морфинга: захват, позиция без клампа (нота/127,
    /// значение), симметричный ли драг ручки, последний клик по узлу дорожки.
    lane_grab: Option<LaneGrab>,
    lane_pos: P2,
    lane_sym: bool,
    lane_click: Option<(usize, f64)>,
    /// Зажатые места дорожек x и карта, по которой посчитаны.
    xclamp: Option<(NoteMap, Vec<Vec<Option<f32>>>)>,
    #[cfg(test)]
    lane_rect: Option<Rect>,
    /// Квадрат графика и полоса клавиш на экране в последнем кадре (для тестов).
    #[cfg(test)]
    rect: Option<Rect>,
    #[cfg(test)]
    kb_rect: Option<Rect>,
}

impl Default for CurveView {
    fn default() -> Self {
        Self {
            note: DEFAULT_KEY,
            sel: None,
            grab: None,
            grab_pos: [0.0; 2],
            sym: false,
            poly: Vec::new(),
            poly_of: None,
            key_drag: None,
            menu_note: DEFAULT_KEY,
            knot_click: None,
            lane_grab: None,
            lane_pos: [0.0; 2],
            lane_sym: false,
            lane_click: None,
            xclamp: None,
            #[cfg(test)]
            lane_rect: None,
            #[cfg(test)]
            rect: None,
            #[cfg(test)]
            kb_rect: None,
        }
    }
}

impl CurveView {
    fn poly(&mut self, shape: &CurveShape) -> &[[f32; 2]] {
        if self.poly_of.as_ref() != Some(shape) {
            self.poly = (0..=POLY_SEGS)
                .map(|j| {
                    let x = j as f32 / POLY_SEGS as f32;
                    [x, shape.eval(x)]
                })
                .collect();
            self.poly_of = Some(shape.clone());
        }
        &self.poly
    }
}

/// История одной кривой (карты нот): снимки на отпускании мыши.
#[derive(Clone, Debug)]
pub struct History<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    /// Последнее зафиксированное состояние.
    committed: T,
}

impl<T: Clone + PartialEq> History<T> {
    pub fn new(cur: &T) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), committed: cur.clone() }
    }

    /// Зафиксировать текущее состояние; `true` — оно отличалось от прежнего.
    pub fn commit(&mut self, cur: &T) -> bool {
        if *cur == self.committed {
            return false;
        }
        let prev = std::mem::replace(&mut self.committed, cur.clone());
        self.undo.push(prev);
        if self.undo.len() > HISTORY_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
        true
    }

    /// Отменить: незафиксированная правка сначала становится шагом истории.
    pub fn undo(&mut self, cur: &mut T) -> bool {
        self.commit(cur);
        match self.undo.pop() {
            Some(prev) => {
                self.redo.push(std::mem::replace(&mut self.committed, prev.clone()));
                *cur = prev;
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self, cur: &mut T) -> bool {
        self.commit(cur);
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(&mut self.committed, next.clone()));
                *cur = next;
                true
            }
            None => false,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// Что изменилось за кадр — вызывающий перезаливает таблицы и сохраняет.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Outcome {
    /// Ноты `lo..=hi`, чьи формы изменились (индекс — [`Slot::idx`]):
    /// перезалить их таблицы.
    pub dirty: [Option<(u8, u8)>; 2],
    /// Переключён формат выхода.
    pub hi_res: bool,
    /// Переключена вкладка.
    pub tab: bool,
    /// Ввод отпущен и карта зафиксирована в истории — момент сохранить.
    pub committed: bool,
}

impl Outcome {
    /// Изменилась хоть одна форма.
    pub fn changed(&self) -> bool {
        self.dirty.iter().any(Option::is_some)
    }

    pub fn any(&self) -> bool {
        self.changed() || self.hi_res || self.tab || self.committed
    }

    /// Отметить ноты `lo..=hi` кривой `slot` (объединение с прежними).
    fn mark(&mut self, slot: Slot, (lo, hi): (u8, u8)) {
        let d = &mut self.dirty[slot.idx()];
        *d = Some(match *d {
            Some((a, b)) => (a.min(lo), b.max(hi)),
            None => (lo, hi),
        });
    }

    fn mark_all(&mut self, slot: Slot) {
        self.mark(slot, (0, LAST_NOTE));
    }

    /// Залить изменённое в банк потока MIDI: таблицы отмеченных нот и формат
    /// выхода.
    pub fn store(&self, set: &CurveSet, bank: &CurveBank) {
        for slot in Slot::ALL {
            if let Some((lo, hi)) = self.dirty[slot.idx()] {
                bank.store_map(slot, set.map(slot), lo, hi);
            }
        }
        if self.hi_res {
            bank.set_hi_res(set.hi_res);
        }
    }
}

/// Правка формы ключевой `k` с графика: вставки и удаления узлов — во всей
/// карте, затем форма целиком. Ноты, которые изменились.
fn key_edit(map: &mut NoteMap, k: usize, shape: CurveShape, ops: &[KnotOp]) -> (u8, u8) {
    for op in ops {
        match *op {
            KnotOp::Insert(seg, t) => {
                map.insert_knot(seg, t);
            }
            KnotOp::Remove(i) => {
                map.remove_knot(i);
            }
        }
    }
    if shape.knots.len() == map.knot_count() {
        map.keys[k].shape = shape;
    }
    if ops.is_empty() {
        map.key_span(k)
    } else {
        (0, LAST_NOTE)
    }
}

/// Правка формы ноты `n` (роль `role`): исключение — только оно; ключевая —
/// [`key_edit`]; промежуточная сначала становится ключевой (точно).
fn note_edit(map: &mut NoteMap, n: u8, role: Role, shape: CurveShape, ops: &[KnotOp]) -> (u8, u8) {
    match role {
        Role::Override(o) => {
            map.overrides[o].shape = shape;
            (n, n)
        }
        Role::Key(k) => key_edit(map, k, shape, ops),
        Role::Morph => {
            let k = map.add_key(n);
            key_edit(map, k, shape, ops)
        }
    }
}

/// Действие на клавиатуре.
#[derive(Clone, Copy, PartialEq, Debug)]
enum KeyAction {
    Select(u8),
    /// Перенести ключевую (индекс) на ноту.
    MoveKey(usize, u8),
    MakeKey(u8),
    MakeOverride(u8),
    RemoveKey(usize),
    RemoveOverride(u8),
}

/// Окно редактора: вкладки Note On / Note Off, график выбранной ноты,
/// клавиатура, библиотека пресетов.
pub struct CurvesUi {
    pub tab: Slot,
    views: [CurveView; 2],
    hist: [History<NoteMap>; 2],
    /// Выбор ноты следует за последней сыгранной.
    pub follow: bool,
    /// Ось дорожки морфинга в редакторе дорожек.
    lane_axis: Axis,
    lib: Library,
    /// Кэш списка библиотеки (`None` — перечитать папку при показе).
    lib_list: Option<Vec<Entry>>,
    lib_name: String,
    /// Первый клик «Delete» взводит, второй удаляет.
    delete_armed: Option<String>,
    notice: Option<(bool, String)>,
    prev_busy: bool,
}

impl CurvesUi {
    pub fn new(set: &CurveSet, tab: Slot, lib: Library) -> Self {
        Self {
            tab,
            views: Default::default(),
            hist: [History::new(&set.note_on), History::new(&set.note_off)],
            follow: false,
            lane_axis: Axis::Y,
            lib,
            lib_list: None,
            lib_name: String::new(),
            delete_armed: None,
            notice: None,
            prev_busy: false,
        }
    }

    /// Кривые заменили извне (загрузка проекта DAW): история и выбор — заново.
    pub fn reset(&mut self, set: &CurveSet) {
        self.hist = [History::new(&set.note_on), History::new(&set.note_off)];
        for v in &mut self.views {
            v.sel = None;
            v.grab = None;
            v.key_drag = None;
            v.lane_grab = None;
        }
    }

    /// Перечитать папку библиотеки при следующем показе.
    pub fn rescan_library(&mut self) {
        self.lib_list = None;
    }

    /// Выбранная нота открытой вкладки.
    pub fn note(&self) -> u8 {
        self.views[self.tab.idx()].note
    }

    /// Нарисовать всё окно. `side` — дополнительная панель вверху справа
    /// (порты MIDI у standalone).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        set: &mut CurveSet,
        bank: &CurveBank,
        side: impl FnOnce(&mut egui::Ui),
    ) -> Outcome {
        let mut out = Outcome::default();
        // Клавиши — только вне жеста мышью (Ctrl+Z посреди драга не отменяет);
        // кнопки — всегда (кликом по кнопке график не тащат).
        let pointer_busy = ctx.is_using_pointer() || ctx.input(|i| i.pointer.any_down());
        let (undo_key, redo_key) = undo_keys(ctx);
        let mut undo_req = undo_key && !pointer_busy;
        let mut redo_req = redo_key && !pointer_busy;
        // Следовать за игрой — не посреди жеста (форма под мышью не меняется).
        if self.follow && !pointer_busy {
            if let Some(n) = bank.last_note(self.tab).filter(|n| (NOTE_LO..=NOTE_HI).contains(n)) {
                self.views[self.tab.idx()].note = n;
            }
        }

        egui::TopBottomPanel::top("curves_top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                for slot in Slot::ALL {
                    let text = egui::RichText::new(slot_name(slot)).color(color(slot));
                    if ui.selectable_label(self.tab == slot, text).clicked() && self.tab != slot {
                        self.tab = slot;
                        out.tab = true;
                    }
                }
                ui.separator();
                if ui
                    .checkbox(&mut set.hi_res, "Hi-res output (14 bit)")
                    .on_hover_text(
                        "On: CC88 (low 7 bits) + note velocity (high 7 bits).\n\
                         Off: plain 7-bit velocity.\n\
                         Incoming CC88 is always decoded.",
                    )
                    .changed()
                {
                    out.hi_res = true;
                }
                ui.separator();
                let h = &self.hist[self.tab.idx()];
                undo_req |= ui.add_enabled(h.can_undo(), egui::Button::new("Undo")).on_hover_text("Ctrl+Z").clicked();
                redo_req |=
                    ui.add_enabled(h.can_redo(), egui::Button::new("Redo")).on_hover_text("Ctrl+Shift+Z").clicked();
                if ui.button("Diagonal").on_hover_text("Reset the selected note to y = x").clicked() {
                    self.set_note_shape(set, CurveShape::linear(), &mut out);
                }
                if ui
                    .button("Reset notes")
                    .on_hover_text("One diagonal for all notes of this curve: key notes and overrides are removed")
                    .clicked()
                {
                    self.reset_notes(set, &mut out);
                }
            });
        });

        egui::SidePanel::right("curves_side").resizable(false).exact_width(250.0).show(ctx, |ui| {
            side(ui);
            self.library_panel(ui, set, &mut out);
            ui.separator();
            let tab = self.tab;
            match (bank.last_note(tab), bank.last_x(tab)) {
                (Some(n), Some(x)) => {
                    let y = bank.eval(tab, n, x);
                    ui.label(format!("Last {}: {}", slot_name(tab), note_label(n)));
                    ui.label(format!("{} → {}", fmt_vel(x), fmt_vel(y)));
                }
                _ => {
                    ui.label(format!("Last {}: —", slot_name(tab)));
                }
            }
            ui.label(egui::RichText::new(format!("Notes processed: {}", bank.notes())).weak());
            ui.add_space(6.0);
            ui.label(egui::RichText::new(HELP).small().weak());
        });

        let mut acts = Vec::new();
        egui::TopBottomPanel::bottom("curves_keys").resizable(false).show(ctx, |ui| {
            let tab = self.tab;
            let view = &mut self.views[tab.idx()];
            ui.horizontal(|ui| {
                match view.sel.filter(|&i| i < set.map(tab).knot_count()) {
                    Some(i) => ui.label(egui::RichText::new(format!("Morph lane of knot {}", i + 1)).strong()),
                    None => ui.label(egui::RichText::new("Morph lanes").strong()),
                };
                for (a, text, tip) in [
                    (Axis::Y, "y", "Output value of the knot across the keyboard"),
                    (Axis::X, "x", "Input position of the knot across the keyboard (all knots; red — crossing, clamped)"),
                ] {
                    if ui.selectable_label(self.lane_axis == a, text).on_hover_text(tip).clicked() {
                        self.lane_axis = a;
                    }
                }
                ui.label(
                    egui::RichText::new(
                        "drag a lane knot — value at that key note; handles bend the morph; double click — linear",
                    )
                    .weak(),
                );
            });
            let le = lane_editor(ui, set.map_mut(tab), self.lane_axis, view, color(tab));
            if let Some(span) = le.dirty {
                out.mark(tab, span);
            }
            if let Some(n) = le.select {
                view.note = n;
            }
            let map = set.map(tab);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(note_label(view.note)).strong());
                ui.label(role_text(map, view.note));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut self.follow, "Follow playing")
                        .on_hover_text("Select the last played note automatically");
                });
            });
            acts = keyboard(ui, map, view, color(tab), bank.last_note(tab));
        });
        for a in acts {
            self.key_action(set, a, &mut out);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let tab = self.tab;
            let view = &mut self.views[tab.idx()];
            let n = view.note;
            let marker = if bank.last_note(tab) == Some(n) { bank.last_x(tab) } else { None };
            let map = set.map_mut(tab);
            let role = role(map, n);
            let (lo, hi) = neighbours(map, n);
            let ghosts: Vec<Vec<[f32; 2]>> = [lo, hi]
                .into_iter()
                .flatten()
                .map(|g| {
                    (0..=GHOST_SEGS)
                        .map(|j| {
                            let x = j as f32 / GHOST_SEGS as f32;
                            [x, bank.eval(tab, g, x)]
                        })
                        .collect()
                })
                .collect();
            let mut shape = match role {
                Role::Override(o) => map.overrides[o].shape.clone(),
                Role::Key(k) => map.keys[k].shape.clone(),
                Role::Morph => map.morph_at(n),
            };
            let edit = curve_editor(ui, &mut shape, view, color(tab), marker, &ghosts);
            if edit.changed {
                out.mark(tab, note_edit(map, n, role, shape, &edit.ops));
            }
        });

        // История: снимок на отпускании ввода, отмена — только вне жеста.
        let released = input_released(ctx, &mut self.prev_busy);
        let tab = self.tab;
        if undo_req || redo_req {
            let map = set.map_mut(tab);
            let h = &mut self.hist[tab.idx()];
            let done = if undo_req { h.undo(map) } else { h.redo(map) };
            if done {
                self.views[tab.idx()].sel = None;
                out.mark_all(tab);
                out.committed = true;
            }
        }
        if released {
            for slot in Slot::ALL {
                out.committed |= self.hist[slot.idx()].commit(set.map(slot));
            }
        }
        out
    }

    /// Одна диагональ на все ноты открытой кривой.
    fn reset_notes(&mut self, set: &mut CurveSet, out: &mut Outcome) {
        *set.map_mut(self.tab) = NoteMap::default();
        self.views[self.tab.idx()].sel = None;
        out.mark_all(self.tab);
    }

    /// Пресет из библиотеки: форма — в выбранную ноту открытой кривой, «всё
    /// сразу» — вместо обеих кривых и формата выхода.
    fn load_preset(&mut self, set: &mut CurveSet, name: String, out: &mut Outcome) {
        let slot = self.tab;
        match self.lib.load(&name) {
            Ok(Preset::Shape(shape)) => {
                self.set_note_shape(set, shape, out);
                let n = self.views[slot.idx()].note;
                self.notice = Some((false, format!("Loaded «{name}» into {} {}", slot_name(slot), note_label(n))));
                self.lib_name = name;
            }
            Ok(Preset::Set(new)) => {
                *set = new;
                for s in Slot::ALL {
                    out.mark_all(s);
                    self.views[s.idx()].sel = None;
                }
                out.hi_res = true;
                self.notice = Some((false, format!("Loaded «{name}»: all notes, Note On and Note Off")));
                self.lib_name = name;
            }
            Err(e) => self.notice = Some((true, format!("«{name}»: {e}"))),
        }
        self.delete_armed = None;
    }

    /// Форма в выбранную ноту (Diagonal, форма из библиотеки): исключению —
    /// как есть, ключевой — с выравниванием числа узлов по карте,
    /// промежуточная сначала становится ключевой.
    fn set_note_shape(&mut self, set: &mut CurveSet, shape: CurveShape, out: &mut Outcome) {
        let slot = self.tab;
        let n = self.views[slot.idx()].note;
        let map = set.map_mut(slot);
        self.views[slot.idx()].sel = None;
        let k = match role(map, n) {
            Role::Override(o) => {
                map.overrides[o].shape = shape;
                out.mark(slot, (n, n));
                return;
            }
            Role::Key(k) => k,
            Role::Morph => map.add_key(n),
        };
        let knots = map.knot_count();
        map.keys[k].shape = map.fit_shape(shape);
        if map.knot_count() == knots {
            out.mark(slot, map.key_span(k));
        } else {
            out.mark_all(slot);
        }
    }

    fn key_action(&mut self, set: &mut CurveSet, a: KeyAction, out: &mut Outcome) {
        let slot = self.tab;
        let map = set.map_mut(slot);
        let view = &mut self.views[slot.idx()];
        match a {
            KeyAction::Select(n) => view.note = n,
            KeyAction::MoveKey(k, n) => {
                // Ключевая ходит между соседями: старое и новое место — в её
                // прежнем диапазоне.
                let span = map.key_span(k);
                view.note = map.move_key(k, n);
                out.mark(slot, span);
            }
            KeyAction::MakeKey(n) => {
                view.note = n;
                match map.override_at(n) {
                    // Исключение становится ключевой со своей формой.
                    Some(o) => {
                        let shape = map.overrides.remove(o).shape;
                        let k = map.add_key(n);
                        let knots = map.knot_count();
                        map.keys[k].shape = map.fit_shape(shape);
                        view.sel = None;
                        if map.knot_count() == knots {
                            out.mark(slot, map.key_span(k));
                        } else {
                            out.mark_all(slot);
                        }
                    }
                    None => {
                        let k = map.add_key(n);
                        out.mark(slot, map.key_span(k));
                    }
                }
            }
            KeyAction::MakeOverride(n) => {
                // Своя форма ноты сохраняется; ключевая выходит из морфинга.
                let shape = map.shape_at(n);
                if let Some(k) = map.key_at(n) {
                    let span = map.key_span(k);
                    if map.remove_key(k) {
                        out.mark(slot, span);
                    }
                }
                map.set_override(n, shape);
                out.mark(slot, (n, n));
                view.note = n;
            }
            KeyAction::RemoveKey(k) => {
                let span = map.key_span(k);
                if map.remove_key(k) {
                    out.mark(slot, span);
                }
            }
            KeyAction::RemoveOverride(n) => {
                if map.remove_override(n) {
                    out.mark(slot, (n, n));
                }
            }
        }
    }

    fn library_panel(&mut self, ui: &mut egui::Ui, set: &mut CurveSet, out: &mut Outcome) {
        ui.label(egui::RichText::new("Presets").strong());
        // Список берётся из кэша на время кадра (без копии); `rescan` — вернуть
        // его в кэш не надо, папку перечитать в следующем кадре.
        let list = match self.lib_list.take() {
            Some(l) => l,
            None => self.lib.scan(),
        };
        let mut rescan = false;
        let mut load: Option<String> = None;
        // Рамка и видимая полоса прокрутки: список читается как прокручиваемый,
        // а не как наезжающий на поле имени.
        ui.group(|ui| {
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for e in &list {
                        let text = if e.set {
                            egui::RichText::new(format!("{} · all", e.name)).color(COL_SET)
                        } else if e.file {
                            egui::RichText::new(&e.name)
                        } else {
                            egui::RichText::new(&e.name).italics()
                        };
                        let mut r = ui.selectable_label(self.lib_name == e.name, text);
                        if e.set {
                            r = r.on_hover_text("Whole setup: Note On and Note Off curves of all notes, output format");
                        }
                        if r.clicked() {
                            load = Some(e.name.clone());
                        }
                    }
                });
        });
        if let Some(name) = load {
            self.load_preset(set, name, out);
        }
        ui.add(egui::TextEdit::singleline(&mut self.lib_name).desired_width(f32::INFINITY).hint_text("preset name"));
        let name = self.lib_name.trim().to_string();
        let is_file = list.iter().any(|e| e.file && e.name == name);
        ui.horizontal(|ui| {
            let ok = library::name_ok(&name);
            let (l_curve, l_all) = if is_file { ("Overwrite curve", "Overwrite all") } else { ("Save curve", "Save all") };
            let n = self.views[self.tab.idx()].note;
            let saved = if ui
                .add_enabled(ok, egui::Button::new(l_curve))
                .on_hover_text(format!("Save the curve of the selected note ({}) as a shape preset", note_label(n)))
                .clicked()
            {
                Some(self.lib.save(&name, &set.map(self.tab).shape_at(n)))
            } else if ui
                .add_enabled(ok, egui::Button::new(l_all))
                .on_hover_text("Save everything: Note On and Note Off curves of all notes and the output format")
                .clicked()
            {
                Some(self.lib.save_set(&name, set))
            } else {
                None
            };
            match saved {
                Some(Ok(())) => {
                    self.notice = Some((false, format!("Saved «{name}»")));
                    rescan = true;
                    self.delete_armed = None;
                }
                Some(Err(e)) => {
                    self.notice = Some((true, format!("«{name}»: {e}")));
                    self.delete_armed = None;
                }
                None => {}
            }
        });
        ui.horizontal(|ui| {
            let armed = self.delete_armed.as_deref() == Some(name.as_str());
            let label = if armed { "Confirm delete" } else { "Delete" };
            if ui
                .add_enabled(is_file, egui::Button::new(label))
                .on_hover_text("Delete the preset file (click twice)")
                .clicked()
            {
                if armed {
                    match self.lib.delete(&name) {
                        Ok(()) => self.notice = Some((false, format!("Deleted «{name}»"))),
                        Err(e) => self.notice = Some((true, format!("«{name}»: {e}"))),
                    }
                    rescan = true;
                    self.delete_armed = None;
                } else {
                    self.delete_armed = Some(name.clone());
                }
            }
            if ui.button("Rescan").on_hover_text("Re-read the preset folder").clicked() {
                rescan = true;
            }
        });
        if let Some((err, text)) = &self.notice {
            let c = if *err { Color32::from_rgb(230, 110, 100) } else { Color32::from_gray(160) };
            ui.colored_label(c, text);
        }
        if !rescan {
            self.lib_list = Some(list);
        }
    }
}

const HELP: &str = "Keyboard: click — select a note, drag a key note — move it, \
right click — key note / override / remove. The graph edits the selected note; \
editing a note between key notes makes it a key note. A knot added or deleted on \
a key note is added or deleted on all key notes.\n\
Click on the curve — add a knot. Drag a knot or handle \
(handle — symmetric, mirrored through the knot); right-drag a handle — handles independently. \
Right/double click a knot — delete; Del — delete selected. \
Shift — fine (×0.1), Ctrl — snap to 1/16, Alt+drag handle — corner, \
Alt+click knot — smooth ↔ corner. Ctrl+Z / Ctrl+Shift+Z — undo / redo. \
Italic presets are built in; «all» presets hold the whole setup.";

/// Строка о выбранной ноте: чем она является в карте.
fn role_text(map: &NoteMap, n: u8) -> String {
    match role(map, n) {
        Role::Override(_) => "override: own curve, outside morphing".into(),
        Role::Key(k) => format!("key note {} of {}", k + 1, map.keys.len()),
        Role::Morph => match neighbours(map, n) {
            (Some(a), Some(b)) => format!("morph between {} and {} — an edit makes it a key note", note_name(a), note_name(b)),
            (Some(a), None) | (None, Some(a)) => format!("same as key note {} — an edit makes it a key note", note_name(a)),
            (None, None) => String::new(),
        },
    }
}

/// Шрифты: в пропорциональном Ubuntu-Light нет `→` (и прочих стрелок) —
/// запасным ставится встроенный в egui моноширинный Hack.
pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let hack = fonts.families[&egui::FontFamily::Monospace][0].clone();
    fonts.families.entry(egui::FontFamily::Proportional).or_default().push(hack);
    ctx.set_fonts(fonts);
}

/// Velocity 0…1 в подписи: 7 бит и 14 бит.
fn fmt_vel(v: f32) -> String {
    format!("{} ({})", (v * 127.0).round(), (v * 16383.0).round())
}

/// Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y (не во время ввода текста): (undo, redo).
fn undo_keys(ctx: &egui::Context) -> (bool, bool) {
    if ctx.wants_keyboard_input() {
        return (false, false);
    }
    ctx.input(|i| {
        let (mut u, mut r) = (false, false);
        for e in &i.events {
            if let egui::Event::Key { key, pressed: true, modifiers, .. } = e {
                let ctrl = modifiers.ctrl || modifiers.command;
                match key {
                    egui::Key::Z if ctrl && !modifiers.shift => u = true,
                    egui::Key::Z if ctrl && modifiers.shift => r = true,
                    egui::Key::Y if ctrl => r = true,
                    _ => {}
                }
            }
        }
        (u, r)
    })
}

/// Ввод только что отпущен (мышь, клавиатурный ввод в поле): момент снимка
/// истории (как `window_input_released` в piano_sim).
fn input_released(ctx: &egui::Context, prev_busy: &mut bool) -> bool {
    let busy = ctx.is_using_pointer() || ctx.wants_keyboard_input() || ctx.input(|i| i.pointer.any_down());
    let released = *prev_busy && !busy;
    *prev_busy = busy;
    released
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum LaneGrab {
    /// Узел дорожки в ключевой (индекс в `keys`).
    Knot(usize),
    In(usize),
    Out(usize),
}

/// Что сделал жест на дорожке за кадр.
#[derive(Default)]
struct LaneEdit {
    /// Изменились ноты lo..=hi.
    dirty: Option<(u8, u8)>,
    /// Выбрать ноту (клик по узлу дорожки).
    select: Option<u8>,
}

/// Где дорожки x соседних узлов пересеклись и узел зажат: [узел][нота −
/// NOTE_LO] → зажатое значение x.
fn x_clamped(map: &NoteMap) -> Vec<Vec<Option<f32>>> {
    let n = map.knot_count();
    let mut out = vec![vec![None; KEYS]; n];
    for note in NOTE_LO..=NOTE_HI {
        let s = map.morph_at(note);
        for (i, row) in out.iter_mut().enumerate().take(n - 1).skip(1) {
            let x = s.knots[i].x;
            if (map.lane_value(Axis::X, i, note as f32) - x).abs() > 1e-6 {
                row[(note - NOTE_LO) as usize] = Some(x);
            }
        }
    }
    out
}

/// Ломаная дорожки узла `i` по оси `axis` в координатах (нота/127, значение):
/// полка до первой ключевой, отрезки Безье, полка после последней.
fn lane_poly(map: &NoteMap, axis: Axis, i: usize) -> Vec<P2> {
    let u = |n: u8| n as f32 / 127.0;
    let first = map.lane_pt(axis, i, 0);
    let mut pts = vec![[u(NOTE_LO).min(first[0]), first[1]]];
    for a in 0..map.keys.len() - 1 {
        let p = map.lane_seg(axis, i, a);
        pts.extend((0..LANE_SEGS).map(|j| crate::curve::shape::bez(&p, j as f32 / LANE_SEGS as f32)));
    }
    let last = map.lane_pt(axis, i, map.keys.len() - 1);
    pts.push(last);
    pts.push([u(NOTE_HI).max(last[0]), last[1]]);
    pts
}

/// Поле дорожки морфинга узла, выбранного на графике (`st.sel`): ось нот —
/// как у клавиатуры. Правит карту сразу (значение узла в ключевой, ручки
/// дорожки, auto).
fn lane_editor(ui: &mut egui::Ui, map: &mut NoteMap, axis: Axis, st: &mut CurveView, col: Color32) -> LaneEdit {
    let mut ed = LaneEdit::default();
    let (resp, painter) = ui.allocate_painter(egui::vec2(ui.available_width(), LANE_H), egui::Sense::click_and_drag());
    let lr = Rect::from_min_max(
        resp.rect.min + egui::vec2(MARGIN_L, 6.0),
        Pos2::new(resp.rect.max.x - MARGIN_R, resp.rect.max.y - 4.0),
    );
    #[cfg(test)]
    {
        st.lane_rect = Some(lr);
    }
    let cw = lr.width() / KEYS as f32;
    let to_s = |p: P2| Pos2::new(lr.left() + (p[0] * 127.0 - NOTE_LO as f32 + 0.5) * cw, lr.bottom() - p[1] * lr.height());
    let knot = st.sel.filter(|&i| i < map.knot_count());
    let sel_key = map.key_at(st.note);
    let now = ui.input(|i| i.time);
    let shift = ui.input(|i| i.modifiers.shift);
    // Узел или ручка выбранной ключевой под точкой `p` на дорожке узла `i`.
    let hit = |map: &NoteMap, i: usize, p: Pos2| -> Option<LaneGrab> {
        if let Some(k) = sel_key {
            if k + 1 < map.keys.len() && to_s(map.lane_seg(axis, i, k)[1]).distance(p) < HIT_HANDLE {
                return Some(LaneGrab::Out(k));
            }
            if k > 0 && to_s(map.lane_seg(axis, i, k - 1)[2]).distance(p) < HIT_HANDLE {
                return Some(LaneGrab::In(k));
            }
        }
        (0..map.keys.len())
            .map(|k| (k, to_s(map.lane_pt(axis, i, k)).distance(p)))
            .filter(|&(_, d)| d < HIT_KNOT)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(k, _)| LaneGrab::Knot(k))
    };
    let tip = |map: &NoteMap, i: usize, g: LaneGrab| match g {
        LaneGrab::Knot(k) => map.lane_pt(axis, i, k),
        LaneGrab::Out(k) => map.lane_seg(axis, i, k)[1],
        LaneGrab::In(k) => map.lane_seg(axis, i, k - 1)[2],
    };

    if let Some(i) = knot {
        // Клик по узлу дорожки — выбрать ключевую; двойной (оба клика по
        // нему) — вернуть линейный морф.
        if resp.clicked() {
            if let Some(LaneGrab::Knot(k)) = resp.interact_pointer_pos().and_then(|p| hit(map, i, p)) {
                let second = resp.double_clicked() && st.lane_click.is_some_and(|(j, t)| j == k && now - t < 0.6);
                if second {
                    map.set_lane_auto(axis, i, k);
                    ed.dirty = Some(map.key_span(k));
                    st.lane_click = None;
                } else {
                    ed.select = Some(map.keys[k].note);
                    st.lane_click = Some((k, now));
                }
            }
        }
        let primary = resp.drag_started_by(egui::PointerButton::Primary);
        let started = primary || resp.drag_started_by(egui::PointerButton::Secondary);
        let origin = ui.input(|i| i.pointer.press_origin());
        if started {
            st.lane_grab = origin
                .and_then(|p| hit(map, i, p))
                .filter(|g| primary || !matches!(g, LaneGrab::Knot(_)));
            st.lane_sym = primary;
            if let Some(g) = st.lane_grab {
                st.lane_pos = tip(map, i, g);
                if let LaneGrab::Knot(k) = g {
                    ed.select = Some(map.keys[k].note);
                }
            }
        }
        let dragging =
            resp.dragged_by(egui::PointerButton::Primary) || resp.dragged_by(egui::PointerButton::Secondary);
        if let (true, Some(g)) = (dragging, st.lane_grab) {
            // В кадре начала — весь ход от точки нажатия: путь до порога драга
            // egui (несколько пикселей) иначе теряется.
            let d = match (started, origin, resp.interact_pointer_pos()) {
                (true, Some(o), Some(p)) => p - o,
                _ => resp.drag_delta(),
            };
            let f = if shift { 0.1 } else { 1.0 };
            st.lane_pos[0] += d.x / (cw * 127.0) * f;
            st.lane_pos[1] -= d.y / lr.height() * f;
            if d != egui::Vec2::ZERO {
                let k = match g {
                    // По горизонтали узел не ходит: ключевую переносят на клавиатуре.
                    LaneGrab::Knot(k) => {
                        let s = &mut map.keys[k].shape;
                        let (kn, v) = (s.knots[i], st.lane_pos[1]);
                        match axis {
                            Axis::Y => s.move_knot(i, kn.x, v),
                            Axis::X => s.move_knot(i, v, kn.y),
                        }
                        k
                    }
                    LaneGrab::Out(k) | LaneGrab::In(k) => {
                        let o = map.lane_pt(axis, i, k);
                        let v = [st.lane_pos[0] - o[0], st.lane_pos[1] - o[1]];
                        map.set_lane_handle(axis, i, k, matches!(g, LaneGrab::Out(_)), v, st.lane_sym);
                        k
                    }
                };
                ed.dirty = Some(map.key_span(k));
            }
        }
        if resp.drag_stopped() {
            st.lane_grab = None;
        }
    }
    let hover = match (knot, st.lane_grab) {
        (Some(i), None) => resp.hover_pos().and_then(|p| hit(map, i, p)),
        _ => None,
    };
    if st.lane_grab.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if hover.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    // ── Отрисовка ──
    painter.rect_filled(lr, 0.0, Color32::from_gray(22));
    let grid = Stroke::new(1.0_f32, Color32::from_gray(40));
    let font = egui::FontId::proportional(10.0);
    for j in 0..=4 {
        let v = j as f32 / 4.0;
        let y = lr.bottom() - v * lr.height();
        painter.line_segment([Pos2::new(lr.left(), y), Pos2::new(lr.right(), y)], grid);
        if j % 2 == 0 {
            painter.text(
                Pos2::new(lr.left() - 6.0, y),
                egui::Align2::RIGHT_CENTER,
                ((v * 127.0).round() as u32).to_string(),
                font.clone(),
                Color32::from_gray(150),
            );
        }
    }
    for n in (NOTE_LO..=NOTE_HI).filter(|n| n % 12 == 0) {
        let x = to_s([n as f32 / 127.0, 0.0]).x;
        painter.line_segment([Pos2::new(x, lr.top()), Pos2::new(x, lr.bottom())], grid);
    }
    for o in &map.overrides {
        let x = to_s([o.note as f32 / 127.0, 0.0]).x;
        painter.line_segment([Pos2::new(x, lr.bottom() - 6.0), Pos2::new(x, lr.bottom())], Stroke::new(2.0_f32, COL_OVERRIDE));
    }
    if (NOTE_LO..=NOTE_HI).contains(&st.note) {
        let x = to_s([st.note as f32 / 127.0, 0.0]).x;
        painter.line_segment(
            [Pos2::new(x, lr.top()), Pos2::new(x, lr.bottom())],
            Stroke::new(1.0_f32, Color32::from_white_alpha(70)),
        );
    }
    painter.rect_stroke(lr, 0.0, Stroke::new(1.0_f32, Color32::from_gray(90)), egui::StrokeKind::Outside);
    let Some(i) = knot else {
        painter.text(
            lr.center(),
            egui::Align2::CENTER_CENTER,
            "select a knot on the graph to see how it moves across the keyboard",
            egui::FontId::proportional(12.0),
            Color32::from_gray(140),
        );
        return ed;
    };
    let nk = map.knot_count();
    let lanes: Vec<usize> = match axis {
        Axis::Y => vec![i],
        Axis::X => (1..nk - 1).collect(),
    };
    for &j in lanes.iter().filter(|&&j| j != i) {
        let pts: Vec<Pos2> = lane_poly(map, axis, j).into_iter().map(to_s).collect();
        painter.add(egui::Shape::line(pts, Stroke::new(1.0_f32, col.gamma_multiply(0.35))));
    }
    let pts: Vec<Pos2> = lane_poly(map, axis, i).into_iter().map(to_s).collect();
    painter.add(egui::Shape::line(pts, Stroke::new(2.0_f32, col)));
    if axis == Axis::X {
        if st.xclamp.as_ref().is_none_or(|(m, _)| m != map) {
            st.xclamp = Some((map.clone(), x_clamped(map)));
        }
        if let Some((_, cl)) = &st.xclamp {
            for row in cl.iter() {
                for (j, x) in row.iter().enumerate() {
                    if let Some(x) = *x {
                        let n = NOTE_LO + j as u8;
                        painter.circle_filled(to_s([n as f32 / 127.0, x]), 2.0, COL_CLAMP);
                    }
                }
            }
        }
    }
    let len = map.keys.len();
    if let Some(k) = sel_key {
        let o = to_s(map.lane_pt(axis, i, k));
        for (show, g) in [(k > 0, LaneGrab::In(k)), (k + 1 < len, LaneGrab::Out(k))] {
            if show {
                let hp = to_s(tip(map, i, g));
                let hot = hover == Some(g) || st.lane_grab == Some(g);
                painter.line_segment([o, hp], Stroke::new(1.0_f32, Color32::from_gray(170)));
                painter.circle_filled(hp, if hot { 5.0 } else { 3.5 }, Color32::from_gray(if hot { 255 } else { 200 }));
            }
        }
    }
    for k in 0..len {
        let p = to_s(map.lane_pt(axis, i, k));
        let hot = hover == Some(LaneGrab::Knot(k)) || st.lane_grab == Some(LaneGrab::Knot(k));
        let r = if hot { 6.0 } else { 4.5 };
        let c = if hot { Color32::WHITE } else { col };
        if Some(k) == sel_key {
            painter.circle_filled(p, r, c);
        } else {
            painter.circle_stroke(p, r, Stroke::new(1.5_f32, c));
        }
    }
    ed
}

/// Полоса клавиатуры A0…C8: клетка на полутон (ось нот общая с дорожками
/// морфинга), чёрные клавиши темнее. Метки: ключевые — кружок цвета кривой,
/// исключения — квадрат, последняя сыгранная — зелёная полоса, выбранная —
/// рамка. Карту не трогает — возвращает действия.
fn keyboard(ui: &mut egui::Ui, map: &NoteMap, st: &mut CurveView, col: Color32, last: Option<u8>) -> Vec<KeyAction> {
    let mut acts = Vec::new();
    let (resp, painter) =
        ui.allocate_painter(egui::vec2(ui.available_width(), KB_H + KB_LABEL_H), egui::Sense::click_and_drag());
    let r = Rect::from_min_max(
        resp.rect.min + egui::vec2(MARGIN_L, 0.0),
        Pos2::new(resp.rect.max.x - MARGIN_R, resp.rect.min.y + KB_H),
    );
    #[cfg(test)]
    {
        st.kb_rect = Some(r);
    }
    let cw = r.width() / KEYS as f32;
    let note_at = |p: Pos2| NOTE_LO + ((p.x - r.left()) / cw).floor().clamp(0.0, (KEYS - 1) as f32) as u8;
    let cell = |n: u8| {
        Rect::from_min_size(Pos2::new(r.left() + (n - NOTE_LO) as f32 * cw, r.top()), egui::vec2(cw, r.height()))
    };

    if resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            acts.push(KeyAction::Select(note_at(p)));
        }
    }
    if resp.drag_started_by(egui::PointerButton::Primary) {
        let origin = ui.input(|i| i.pointer.press_origin());
        st.key_drag = origin.and_then(|p| map.key_at(note_at(p)));
    }
    if resp.dragged_by(egui::PointerButton::Primary) {
        if let Some(n) = resp.interact_pointer_pos().map(note_at) {
            match st.key_drag {
                Some(k) if map.keys.get(k).is_some_and(|key| key.note != n) => acts.push(KeyAction::MoveKey(k, n)),
                Some(_) => {}
                None if n != st.note => acts.push(KeyAction::Select(n)),
                None => {}
            }
        }
    }
    if resp.drag_stopped() {
        st.key_drag = None;
    }
    if resp.secondary_clicked() {
        if let Some(n) = resp.interact_pointer_pos().map(note_at) {
            st.menu_note = n;
            acts.push(KeyAction::Select(n));
        }
    }
    let menu_note = st.menu_note;
    resp.context_menu(|ui| {
        let n = menu_note;
        ui.label(egui::RichText::new(note_label(n)).strong());
        let (key, ov) = (map.key_at(n), map.override_at(n).is_some());
        if key.is_none() && ui.button("Make key note").clicked() {
            acts.push(KeyAction::MakeKey(n));
            ui.close_menu();
        }
        if !ov && ui.button("Make override").on_hover_text("Own curve, outside morphing").clicked() {
            acts.push(KeyAction::MakeOverride(n));
            ui.close_menu();
        }
        if let Some(k) = key {
            if ui.add_enabled(map.keys.len() > 1, egui::Button::new("Remove key note")).clicked() {
                acts.push(KeyAction::RemoveKey(k));
                ui.close_menu();
            }
        }
        if ov && ui.button("Remove override").clicked() {
            acts.push(KeyAction::RemoveOverride(n));
            ui.close_menu();
        }
    });
    let hover = resp.hover_pos().filter(|p| r.contains(*p)).map(note_at);
    if st.key_drag.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    } else if hover.is_some_and(|n| map.key_at(n).is_some()) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    // ── Отрисовка ──
    let font = egui::FontId::proportional(10.0);
    for n in NOTE_LO..=NOTE_HI {
        let c = cell(n).shrink2(egui::vec2(0.5, 0.0));
        let black = matches!(n % 12, 1 | 3 | 6 | 8 | 10);
        let g = match (black, hover == Some(n)) {
            (true, false) => 50,
            (true, true) => 80,
            (false, false) => 150,
            (false, true) => 185,
        };
        painter.rect_filled(c, 1.0, Color32::from_gray(g));
        if n % 12 == 0 {
            painter.text(
                Pos2::new(c.left(), r.bottom() + 1.0),
                egui::Align2::LEFT_TOP,
                note_name(n),
                font.clone(),
                Color32::from_gray(150),
            );
        }
        if last == Some(n) {
            let bar = Rect::from_min_max(Pos2::new(c.left(), c.bottom() - 4.0), c.max);
            painter.rect_filled(bar, 0.0, COL_MARKER);
        }
    }
    let rad = (cw * 0.4).clamp(2.0, 4.5);
    for key in &map.keys {
        let c = cell(key.note);
        let p = Pos2::new(c.center().x, c.top() + rad + 2.0);
        painter.circle_filled(p, rad, col);
        painter.circle_stroke(p, rad, Stroke::new(1.0_f32, Color32::from_gray(20)));
    }
    for o in &map.overrides {
        let c = cell(o.note);
        let q = Rect::from_center_size(Pos2::new(c.center().x, c.center().y + 3.0), egui::vec2(2.0 * rad, 2.0 * rad));
        painter.rect_filled(q, 0.0, COL_OVERRIDE);
        painter.rect_stroke(q, 0.0, Stroke::new(1.0_f32, Color32::from_gray(20)), egui::StrokeKind::Outside);
    }
    if (NOTE_LO..=NOTE_HI).contains(&st.note) {
        painter.rect_stroke(cell(st.note), 1.0, Stroke::new(2.0_f32, Color32::WHITE), egui::StrokeKind::Inside);
    }
    acts
}

/// График одной кривой с жестами. `ghosts` — ломаные соседних ключевых нот
/// (единичный квадрат), рисуются бледно под кривой.
fn curve_editor(
    ui: &mut egui::Ui,
    shape: &mut CurveShape,
    st: &mut CurveView,
    col: Color32,
    marker: Option<f32>,
    ghosts: &[Vec<[f32; 2]>],
) -> Edit {
    let mut ed = Edit::default();
    let mods = ui.input(|i| i.modifiers);
    let snap = mods.ctrl || mods.command;
    let avail = ui.available_size();
    let (resp, painter) = ui.allocate_painter(avail, egui::Sense::click_and_drag());
    let pr = Rect::from_min_max(
        resp.rect.min + egui::vec2(MARGIN_L, 10.0),
        resp.rect.max - egui::vec2(MARGIN_R, 26.0),
    );
    #[cfg(test)]
    {
        st.rect = Some(pr);
    }
    let to_s = |u: [f32; 2]| Pos2::new(pr.left() + u[0] * pr.width(), pr.bottom() - u[1] * pr.height());
    let to_u = |p: Pos2| [(p.x - pr.left()) / pr.width(), (pr.bottom() - p.y) / pr.height()];
    if st.sel.is_some_and(|i| i >= shape.knots.len()) {
        st.sel = None;
    }
    // Длина — из самой формы: Delete ниже может убрать узел до клика.
    let hit = |shape: &CurveShape, sel: Option<usize>, p: Pos2| -> Option<Grab> {
        let n = shape.knots.len();
        if let Some(i) = sel.filter(|&i| i < n) {
            let k = shape.pt(i);
            let ho = shape.h_out(i);
            let hi = shape.h_in(i);
            if i + 1 < n && to_s([k[0] + ho[0], k[1] + ho[1]]).distance(p) < HIT_HANDLE {
                return Some(Grab::Out(i));
            }
            if i > 0 && to_s([k[0] + hi[0], k[1] + hi[1]]).distance(p) < HIT_HANDLE {
                return Some(Grab::In(i));
            }
        }
        (0..n)
            .map(|i| (i, to_s(shape.pt(i)).distance(p)))
            .filter(|&(_, d)| d < HIT_KNOT)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| Grab::Knot(i))
    };
    let near_curve = |shape: &CurveShape, p: Pos2| {
        let u = to_u(p);
        (0.0..=1.0).contains(&u[0]) && (to_s([u[0], shape.eval(u[0])]).y - p.y).abs() < HIT_KNOT
    };
    let grab_target_pos = |shape: &CurveShape, g: Grab| match g {
        Grab::Knot(i) => shape.pt(i),
        Grab::Out(i) => {
            let (k, v) = (shape.pt(i), shape.h_out(i));
            [k[0] + v[0], k[1] + v[1]]
        }
        Grab::In(i) => {
            let (k, v) = (shape.pt(i), shape.h_in(i));
            [k[0] + v[0], k[1] + v[1]]
        }
    };
    // Вставка узла в точку кривой с абсциссой x (форма не меняется) — с
    // записью операции для всей карты.
    let insert = |shape: &mut CurveShape, ed: &mut Edit, x: f32| {
        let (seg, t) = shape.locate(x);
        let i = shape.split(seg, t)?;
        ed.ops.push(KnotOp::Insert(seg, t));
        ed.changed = true;
        Some(i)
    };
    let remove = |shape: &mut CurveShape, ed: &mut Edit, i: usize| {
        if i > 0 && i + 1 < shape.knots.len() {
            shape.remove(i);
            ed.ops.push(KnotOp::Remove(i));
            ed.changed = true;
        }
    };

    // Удаление: ПКМ / двойной клик по узлу, Delete — выбранный.
    let now = ui.input(|i| i.time);
    let mut removed = false;
    if resp.secondary_clicked() || resp.double_clicked() {
        if let Some(Grab::Knot(i)) = resp.interact_pointer_pos().and_then(|p| hit(shape, None, p)) {
            let first_here = st.knot_click.is_some_and(|(j, t)| j == i && now - t < 0.6);
            if resp.secondary_clicked() || first_here {
                remove(shape, &mut ed, i);
                st.sel = None;
                st.knot_click = None;
                removed = true;
            }
        }
    }
    let del = !ui.ctx().wants_keyboard_input()
        && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
    if let (true, Some(i)) = (del, st.sel) {
        if i > 0 && i + 1 < shape.knots.len() {
            remove(shape, &mut ed, i);
            st.sel = None;
        }
    }
    // Клик: выбрать узел / Alt — гладкий↔угол / по кривой — новый узел.
    if resp.clicked() && !removed {
        if let Some(p) = resp.interact_pointer_pos() {
            match hit(shape, st.sel, p) {
                Some(Grab::Knot(i)) if mods.alt => {
                    shape.toggle_smooth(i);
                    st.sel = Some(i);
                    ed.changed = true;
                }
                Some(Grab::Knot(i)) => {
                    st.sel = Some(i);
                    st.knot_click = Some((i, now));
                }
                Some(_) => {}
                None if near_curve(shape, p) => {
                    if let Some(i) = insert(shape, &mut ed, to_u(p)[0]) {
                        st.sel = Some(i);
                    }
                }
                None => st.sel = None,
            }
        }
    }
    // ПКМ-драг ручки выбранного узла — ручки по отдельности (не симметрично).
    if resp.drag_started_by(egui::PointerButton::Secondary) {
        let origin = ui.input(|i| i.pointer.press_origin());
        st.grab = origin
            .and_then(|p| hit(shape, st.sel, p))
            .filter(|g| matches!(g, Grab::In(_) | Grab::Out(_)));
        st.sym = false;
        if let Some(g) = st.grab {
            st.grab_pos = grab_target_pos(shape, g);
        }
    }
    // Драг: захват по точке НАЖАТИЯ (drag_started приходит после порога).
    if resp.drag_started_by(egui::PointerButton::Primary) {
        st.sym = true;
        let origin = ui.input(|i| i.pointer.press_origin());
        st.grab = origin.and_then(|p| match hit(shape, st.sel, p) {
            Some(g) => Some(g),
            None if near_curve(shape, p) => insert(shape, &mut ed, to_u(p)[0]).map(Grab::Knot),
            None => None,
        });
        if let Some(g) = st.grab {
            st.grab_pos = grab_target_pos(shape, g);
            if let Grab::Knot(i) = g {
                st.sel = Some(i);
            }
        }
    }
    let dragging =
        resp.dragged_by(egui::PointerButton::Primary) || resp.dragged_by(egui::PointerButton::Secondary);
    if let (true, Some(g)) = (dragging, st.grab) {
        let d = resp.drag_delta();
        let k = if mods.shift { 0.1 } else { 1.0 };
        st.grab_pos[0] += d.x / pr.width() * k;
        st.grab_pos[1] -= d.y / pr.height() * k;
        let mut p = st.grab_pos;
        if snap {
            p = [(p[0] / SNAP).round() * SNAP, (p[1] / SNAP).round() * SNAP];
        }
        if d != egui::Vec2::ZERO || snap {
            let before = shape.clone();
            match g {
                Grab::Knot(i) => shape.move_knot(i, p[0], p[1]),
                // Alt ломает узел в угол — симметрии там нет.
                Grab::Out(i) if st.sym && !mods.alt => shape.set_handle_sym(i, true, p),
                Grab::In(i) if st.sym && !mods.alt => shape.set_handle_sym(i, false, p),
                Grab::Out(i) => shape.set_handle(i, true, p, mods.alt),
                Grab::In(i) => shape.set_handle(i, false, p, mods.alt),
            }
            ed.changed |= *shape != before;
        }
    }
    if resp.drag_stopped() {
        st.grab = None;
    }
    let hover = if st.grab.is_none() { resp.hover_pos().and_then(|p| hit(shape, st.sel, p)) } else { None };
    if st.grab.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if hover.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    // ── Отрисовка ──
    painter.rect_filled(pr, 0.0, Color32::from_gray(22));
    let grid = Stroke::new(1.0_f32, Color32::from_gray(48));
    let label_col = Color32::from_gray(150);
    let font = egui::FontId::proportional(11.0);
    for j in 0..=4 {
        let f = j as f32 / 4.0;
        painter.line_segment([to_s([f, 0.0]), to_s([f, 1.0])], grid);
        painter.line_segment([to_s([0.0, f]), to_s([1.0, f])], grid);
        let midi = (f * 127.0).round() as u32;
        painter.text(to_s([f, 0.0]) + egui::vec2(0.0, 4.0), egui::Align2::CENTER_TOP, midi.to_string(), font.clone(), label_col);
        painter.text(to_s([0.0, f]) - egui::vec2(6.0, 0.0), egui::Align2::RIGHT_CENTER, midi.to_string(), font.clone(), label_col);
    }
    painter.text(
        to_s([1.0, 0.0]) + egui::vec2(0.0, 14.0),
        egui::Align2::RIGHT_TOP,
        "input velocity",
        font.clone(),
        label_col,
    );
    painter.rect_stroke(pr, 0.0, Stroke::new(1.0_f32, Color32::from_gray(90)), egui::StrokeKind::Outside);
    painter.line_segment([to_s([0.0, 0.0]), to_s([1.0, 1.0])], Stroke::new(1.0_f32, Color32::from_gray(60)));
    for g in ghosts {
        let pts: Vec<Pos2> = g.iter().map(|&u| to_s(u)).collect();
        painter.add(egui::Shape::line(pts, Stroke::new(1.0_f32, col.gamma_multiply(0.3))));
    }
    let pts: Vec<Pos2> = st.poly(shape).iter().map(|&u| to_s(u)).collect();
    painter.add(egui::Shape::line(pts, Stroke::new(2.0_f32, col)));
    // Velocity последней ноты (если сыграна выбранная): где на кривой играли.
    if let Some(x) = marker {
        let p = to_s([x, shape.eval(x)]);
        painter.line_segment([to_s([x, 0.0]), to_s([x, 1.0])], Stroke::new(1.0_f32, COL_MARKER.gamma_multiply(0.6)));
        painter.circle_filled(p, 4.0, COL_MARKER);
    }
    // Ручки выбранного узла.
    if let Some(i) = st.sel {
        let k = to_s(shape.pt(i));
        let hstroke = Stroke::new(1.0_f32, Color32::from_gray(170));
        for (show, v, g) in [
            (i > 0, shape.h_in(i), Grab::In(i)),
            (i + 1 < shape.knots.len(), shape.h_out(i), Grab::Out(i)),
        ] {
            if show {
                let hp = to_s([shape.pt(i)[0] + v[0], shape.pt(i)[1] + v[1]]);
                let hot = hover == Some(g) || st.grab == Some(g);
                painter.line_segment([k, hp], hstroke);
                painter.circle_filled(hp, if hot { 5.0 } else { 3.5 }, Color32::from_gray(if hot { 255 } else { 200 }));
            }
        }
    }
    for (i, kn) in shape.knots.iter().enumerate() {
        let p = to_s([kn.x, kn.y]);
        let selected = st.sel == Some(i);
        let hot = hover == Some(Grab::Knot(i)) || st.grab == Some(Grab::Knot(i));
        let r = if hot { 6.5 } else { 5.0 };
        let c = if hot { Color32::WHITE } else { col };
        // Угловой узел — квадрат, гладкий — круг.
        if kn.smooth {
            if selected {
                painter.circle_filled(p, r, c);
            } else {
                painter.circle_stroke(p, r, Stroke::new(1.5_f32, c));
            }
        } else {
            let rect = Rect::from_center_size(p, egui::vec2(2.0 * r - 1.0, 2.0 * r - 1.0));
            if selected {
                painter.rect_filled(rect, 0.0, c);
            } else {
                painter.rect_stroke(rect, 0.0, Stroke::new(1.5_f32, c), egui::StrokeKind::Middle);
            }
        }
    }
    // Курсор: чтение кривой под мышью.
    if let Some(p) = resp.hover_pos().filter(|p| pr.contains(*p)) {
        let x = to_u(p)[0].clamp(0.0, 1.0);
        let y = shape.eval(x);
        painter.text(
            pr.left_top() + egui::vec2(6.0, 4.0),
            egui::Align2::LEFT_TOP,
            format!("in {} → out {}", fmt_vel(x), fmt_vel(y)),
            font,
            Color32::from_gray(210),
        );
    }
    ed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Все символы подписей GUI есть в шрифтах (иначе — «квадратики»).
    #[test]
    fn gui_glyphs_present() {
        let ctx = egui::Context::default();
        setup_fonts(&ctx);
        let _ = ctx.run(Default::default(), |_| {});
        let text = format!("— none — → ↔ × «» … ≈ · # {HELP}");
        let font = egui::FontId::proportional(11.0);
        let missing: String =
            ctx.fonts(|f| text.chars().filter(|&c| !c.is_control() && !f.has_glyph(&font, c)).collect());
        assert!(missing.is_empty(), "нет глифов: {missing:?}");
    }

    fn bent() -> CurveShape {
        let mut s = CurveShape::linear();
        let i = s.insert_at(0.4).unwrap();
        s.move_knot(i, 0.4, 0.8);
        s
    }

    #[test]
    fn undo_redo_restores_shapes_bit_exact() {
        let a = CurveShape::linear();
        let mut cur = a.clone();
        let mut h = History::new(&cur);
        assert!(!h.commit(&cur));
        cur = bent();
        let b = cur.clone();
        assert!(h.commit(&cur));
        cur.move_knot(1, 0.5, 0.1);
        let c = cur.clone();
        // Незафиксированная правка (c) сама становится шагом истории.
        assert!(h.undo(&mut cur));
        assert_eq!(cur, b);
        assert!(h.undo(&mut cur));
        assert_eq!(cur, a);
        assert!(!h.undo(&mut cur));
        assert!(h.redo(&mut cur));
        assert_eq!(cur, b);
        assert!(h.redo(&mut cur));
        assert_eq!(cur, c);
        assert!(!h.redo(&mut cur));
        // Новая правка после отмены обрезает повтор.
        h.undo(&mut cur);
        cur = CurveShape::linear();
        cur.move_knot(0, 0.0, 0.3);
        h.commit(&cur);
        assert!(!h.can_redo());
    }

    #[test]
    fn history_depth_is_bounded() {
        let mut cur = CurveShape::linear();
        let mut h = History::new(&cur);
        for j in 0..(HISTORY_DEPTH + 20) {
            cur.move_knot(0, 0.0, j as f32 / 1000.0 + 0.001);
            h.commit(&cur);
        }
        let mut n = 0;
        while h.undo(&mut cur) {
            n += 1;
        }
        assert_eq!(n, HISTORY_DEPTH);
    }

    #[test]
    fn note_names() {
        assert_eq!(
            [21u8, 60, 61, 108, 0, 127].map(note_name),
            ["A0", "C4", "C#4", "C8", "C-1", "G9"].map(String::from)
        );
    }

    /// Окно без экрана: кадры egui с синтетическим вводом. Таблицы банка
    /// заливаются по `Outcome`, как у плагина и standalone.
    struct Harness {
        ctx: egui::Context,
        ui: CurvesUi,
        set: CurveSet,
        bank: CurveBank,
        t: f64,
    }

    impl Harness {
        fn new() -> Self {
            Self::with(CurveSet::default())
        }

        fn with(set: CurveSet) -> Self {
            let bank = CurveBank::new();
            set.apply(&bank);
            let mut h = Self {
                ctx: egui::Context::default(),
                ui: CurvesUi::new(&set, Slot::NoteOn, Library::new(None)),
                set,
                bank,
                t: 0.0,
            };
            h.frame(vec![], egui::Modifiers::NONE);
            h.frame(vec![], egui::Modifiers::NONE);
            h
        }

        fn frame(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers) -> Outcome {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 800.0))),
                events,
                modifiers,
                time: Some(self.t),
                ..Default::default()
            };
            self.t += 1.0 / 60.0;
            let mut out = Outcome::default();
            let _ = self.ctx.run(raw, |ctx| out = self.ui.show(ctx, &mut self.set, &self.bank, |_| {}));
            out.store(&self.set, &self.bank);
            out
        }

        /// Форма ключевой ноты по умолчанию (60) карты Note On.
        fn on(&self) -> &CurveShape {
            &self.set.note_on.keys[self.set.note_on.key_at(DEFAULT_KEY).unwrap()].shape
        }

        /// Точка единичного квадрата → экран.
        fn at(&self, u: [f32; 2]) -> Pos2 {
            let r = self.ui.views[self.ui.tab.idx()].rect.unwrap();
            Pos2::new(r.left() + u[0] * r.width(), r.bottom() - u[1] * r.height())
        }

        /// Центр клавиши ноты `n` на экране.
        fn key(&self, n: u8) -> Pos2 {
            let r = self.ui.views[self.ui.tab.idx()].kb_rect.unwrap();
            let cw = r.width() / KEYS as f32;
            Pos2::new(r.left() + ((n - NOTE_LO) as f32 + 0.5) * cw, r.center().y)
        }

        /// Точка поля дорожки (нота, значение) → экран.
        fn lane(&self, p: P2) -> Pos2 {
            let r = self.ui.views[self.ui.tab.idx()].lane_rect.unwrap();
            let cw = r.width() / KEYS as f32;
            Pos2::new(r.left() + (p[0] * 127.0 - NOTE_LO as f32 + 0.5) * cw, r.bottom() - p[1] * r.height())
        }

        fn button(&mut self, p: Pos2, button: egui::PointerButton, pressed: bool) -> Outcome {
            let e = egui::Event::PointerButton { pos: p, button, pressed, modifiers: egui::Modifiers::NONE };
            self.frame(vec![e], egui::Modifiers::NONE)
        }

        /// Клик; накопленный `Outcome` всех кадров, включая отпускание.
        fn click(&mut self, p: Pos2, button: egui::PointerButton) -> Outcome {
            let mut acc = self.frame(vec![egui::Event::PointerMoved(p)], egui::Modifiers::NONE);
            for o in [self.button(p, button, true), self.button(p, button, false), self.frame(vec![], egui::Modifiers::NONE)] {
                acc = merge(acc, o);
            }
            acc
        }

        fn drag(&mut self, from: Pos2, to: Pos2) -> Outcome {
            self.drag_by(from, to, egui::PointerButton::Primary)
        }

        fn drag_by(&mut self, from: Pos2, to: Pos2, button: egui::PointerButton) -> Outcome {
            let mut acc = self.frame(vec![egui::Event::PointerMoved(from)], egui::Modifiers::NONE);
            acc = merge(acc, self.button(from, button, true));
            for j in 1..=10 {
                let p = from + (to - from) * (j as f32 / 10.0);
                acc = merge(acc, self.frame(vec![egui::Event::PointerMoved(p)], egui::Modifiers::NONE));
            }
            acc = merge(acc, self.button(to, button, false));
            merge(acc, self.frame(vec![], egui::Modifiers::NONE))
        }

        fn key_press(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> Outcome {
            let ev = |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
            let a = self.frame(vec![ev(true)], modifiers);
            merge(a, self.frame(vec![ev(false)], egui::Modifiers::NONE))
        }

        /// Таблицы банка совпадают с формами карты на всех нотах обеих кривых
        /// (заливка по `Outcome::dirty` ничего не пропустила).
        fn assert_bank_in_sync(&self) {
            for slot in Slot::ALL {
                for n in 0..NOTES as u8 {
                    let s = self.set.map(slot).shape_at(n);
                    for x in [0.05f32, 0.3, 0.5, 0.77, 0.95] {
                        let (b, e) = (self.bank.eval(slot, n, x), s.eval(x));
                        assert!((b - e).abs() < 1e-4, "{slot:?} нота {n} x={x}: банк {b}, форма {e}");
                    }
                }
            }
        }
    }

    fn merge(a: Outcome, b: Outcome) -> Outcome {
        let mut m = Outcome {
            dirty: a.dirty,
            hi_res: a.hi_res || b.hi_res,
            tab: a.tab || b.tab,
            committed: a.committed || b.committed,
        };
        for slot in Slot::ALL {
            if let Some(r) = b.dirty[slot.idx()] {
                m.mark(slot, r);
            }
        }
        m
    }

    /// Карта Note On с ключевыми 30 (тише) и 90 (диагональ) по 3 узла.
    fn two_keys() -> CurveSet {
        let mut set = CurveSet::default();
        let m = &mut set.note_on;
        m.keys[0].note = 30;
        m.keys[0].shape.move_knot(1, 1.0, 0.6);
        m.insert_knot(0, 0.5);
        let k = m.add_key(90);
        m.keys[k].shape = m.fit_shape(CurveShape::linear());
        set
    }

    #[test]
    fn click_on_curve_adds_knot_without_changing_shape() {
        let mut h = Harness::new();
        let before: Vec<f32> = (0..=20).map(|j| h.on().eval(j as f32 / 20.0)).collect();
        let out = h.click(h.at([0.5, 0.5]), egui::PointerButton::Primary);
        assert_eq!(h.on().knots.len(), 3);
        assert!(out.dirty[0].is_some() && out.committed && out.dirty[1].is_none());
        for (j, y0) in before.iter().enumerate() {
            assert!((h.on().eval(j as f32 / 20.0) - y0).abs() < 1e-4);
        }
        assert!(h.ui.hist[0].can_undo());
        h.assert_bank_in_sync();
    }

    #[test]
    fn drag_knot_then_undo_redo_and_delete() {
        let mut h = Harness::new();
        h.click(h.at([0.5, 0.5]), egui::PointerButton::Primary);
        let inserted = h.set.note_on.clone();
        let out = h.drag(h.at([0.5, 0.5]), h.at([0.5, 0.8]));
        assert!(out.dirty[0].is_some() && out.committed);
        let k = h.on().knots[1];
        assert!((k.y - 0.8).abs() < 0.02 && (k.x - 0.5).abs() < 0.02, "узел {k:?}");
        let dragged = h.set.note_on.clone();
        h.key_press(egui::Key::Z, egui::Modifiers::CTRL);
        assert_eq!(h.set.note_on, inserted);
        h.key_press(egui::Key::Z, egui::Modifiers::CTRL | egui::Modifiers::SHIFT);
        assert_eq!(h.set.note_on, dragged);
        // ПКМ по узлу — удалить; крайние узлы неудаляемы.
        h.click(h.at([0.5, k.y]), egui::PointerButton::Secondary);
        assert_eq!(h.on().knots.len(), 2);
        h.click(h.at([0.0, 0.0]), egui::PointerButton::Secondary);
        assert_eq!(h.on().knots.len(), 2);
        h.assert_bank_in_sync();
    }

    /// Двойной клик по узлу удаляет его; клик по клавише и сразу по узлу —
    /// только выбор (egui считает это двойным кликом).
    #[test]
    fn double_click_deletes_only_after_click_on_same_knot() {
        let mut h = Harness::new();
        h.click(h.at([0.5, 0.5]), egui::PointerButton::Primary);
        h.t += 1.0;
        h.click(h.key(60), egui::PointerButton::Primary);
        h.click(h.at([0.5, 0.5]), egui::PointerButton::Primary);
        assert_eq!(h.on().knots.len(), 3, "клик по клавише + клик по узлу удалил узел");
        // Пауза: следующий быстрый клик egui счёл бы тройным.
        h.t += 1.0;
        h.click(h.at([0.5, 0.5]), egui::PointerButton::Primary);
        h.click(h.at([0.5, 0.5]), egui::PointerButton::Primary);
        assert_eq!(h.on().knots.len(), 2, "двойной клик по узлу не удалил его");
    }

    #[test]
    fn handle_drag_lmb_symmetric_rmb_independent() {
        let mut h = Harness::new();
        h.click(h.at([0.5, 0.5]), egui::PointerButton::Primary);
        let tip = |s: &CurveShape| {
            let (k, v) = (s.pt(1), s.h_out(1));
            [k[0] + v[0], k[1] + v[1]]
        };
        let len = |v: [f32; 2]| v[0].hypot(v[1]);
        // ПКМ: исходящая ручка длиннее, входящая — та же длина, на одной прямой.
        let len_in = len(h.on().h_in(1));
        let out = h.drag_by(h.at(tip(h.on())), h.at([0.75, 0.6]), egui::PointerButton::Secondary);
        assert!(out.dirty[0].is_some() && out.committed);
        assert_eq!(h.on().knots.len(), 3, "ПКМ-драг ручки удалил узел");
        let (ho, hi) = (h.on().h_out(1), h.on().h_in(1));
        assert!((len(ho) - len_in).abs() > 0.03, "исходящая не сдвинулась: {ho:?}");
        assert!((len(hi) - len_in).abs() < 1e-4, "ПКМ изменил длину входящей: {hi:?}");
        assert!((ho[0] * hi[1] - ho[1] * hi[0]).abs() < 1e-4, "ручки не на одной прямой");
        // ЛКМ: входящая — точное зеркало исходящей.
        h.drag(h.at(tip(h.on())), h.at([0.7, 0.75]));
        let (ho, hi) = (h.on().h_out(1), h.on().h_in(1));
        assert!((ho[0] - 0.2).abs() < 0.02 && (ho[1] - 0.25).abs() < 0.02, "исходящая {ho:?}");
        assert!((ho[0] + hi[0]).abs() < 1e-6 && (ho[1] + hi[1]).abs() < 1e-6, "{ho:?} {hi:?}");
    }

    #[test]
    fn ctrl_z_during_drag_is_ignored() {
        let mut h = Harness::new();
        h.click(h.at([0.5, 0.5]), egui::PointerButton::Primary);
        let inserted = h.set.note_on.clone();
        let from = h.at([0.5, 0.5]);
        h.frame(vec![egui::Event::PointerMoved(from)], egui::Modifiers::NONE);
        h.button(from, egui::PointerButton::Primary, true);
        for j in 1..=5 {
            h.frame(vec![egui::Event::PointerMoved(from + egui::vec2(0.0, -8.0 * j as f32))], egui::Modifiers::NONE);
        }
        assert_ne!(h.set.note_on, inserted);
        let z = egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };
        // Ctrl посреди драга — привязка к сетке (y → 9/16), но не отмена.
        h.frame(vec![z], egui::Modifiers::CTRL);
        assert_ne!(h.set.note_on, inserted, "Ctrl+Z посреди драга отменил правку");
        assert_eq!(h.on().knots[1].y, 9.0 / 16.0);
        h.button(from + egui::vec2(0.0, -40.0), egui::PointerButton::Primary, false);
        h.frame(vec![], egui::Modifiers::NONE);
        // После отпускания — обычная отмена: назад к форме до драга.
        h.key_press(egui::Key::Z, egui::Modifiers::CTRL);
        assert_eq!(h.set.note_on, inserted);
    }

    #[test]
    fn tabs_edit_their_own_curve() {
        let mut h = Harness::new();
        h.ui.tab = Slot::NoteOff;
        h.frame(vec![], egui::Modifiers::NONE);
        let out = h.drag(h.at([1.0, 1.0]), h.at([1.0, 0.6]));
        assert!(out.dirty[1].is_some() && out.dirty[0].is_none());
        assert!((h.set.note_off.keys[0].shape.knots[1].y - 0.6).abs() < 0.02);
        assert_eq!(h.set.note_on, NoteMap::default());
        assert!(!h.ui.hist[0].can_undo() && h.ui.hist[1].can_undo());
        h.assert_bank_in_sync();
    }

    #[test]
    fn click_on_keyboard_selects_note() {
        let mut h = Harness::new();
        assert_eq!(h.ui.note(), DEFAULT_KEY);
        for n in [NOTE_LO, 72, NOTE_HI] {
            let out = h.click(h.key(n), egui::PointerButton::Primary);
            assert_eq!(h.ui.note(), n);
            assert!(!out.changed());
        }
        assert_eq!(h.set, CurveSet::default());
    }

    /// Первая правка промежуточной ноты делает её ключевой: сама вставка
    /// ничего не меняет, правка меняет только ноты между соседними ключевыми.
    #[test]
    fn edit_on_intermediate_note_creates_key() {
        let mut h = Harness::with(two_keys());
        let before = h.set.note_on.clone();
        h.click(h.key(60), egui::PointerButton::Primary);
        // Клик по узлу — только выбор, ключевой не появляется.
        let mid = before.shape_at(60);
        h.click(h.at(mid.pt(1)), egui::PointerButton::Primary);
        assert_eq!(h.set.note_on, before);
        let out = h.drag(h.at(mid.pt(1)), h.at([mid.pt(1)[0], 0.2]));
        let m = &h.set.note_on;
        assert_eq!(m.keys.iter().map(|k| k.note).collect::<Vec<_>>(), vec![30, 60, 90]);
        assert!((m.keys[1].shape.knots[1].y - 0.2).abs() < 0.02);
        assert_eq!(out.dirty[0], Some((30, 90)));
        for n in (0..30).chain(90..NOTES as u8) {
            assert_eq!(m.shape_at(n), before.shape_at(n), "нота {n}");
        }
        h.assert_bank_in_sync();
        // Отмена — карта бит-в-бит прежняя (и ключевой 60 нет).
        h.key_press(egui::Key::Z, egui::Modifiers::CTRL);
        assert_eq!(h.set.note_on, before);
        h.assert_bank_in_sync();
    }

    /// Узел, добавленный кликом на ключевой, появляется во всех ключевых;
    /// формы ключевых не меняются; удаление — тоже из всех; отмена — бит-в-бит.
    #[test]
    fn knot_insert_and_delete_apply_to_all_keys() {
        let mut h = Harness::with(two_keys());
        let start = h.set.note_on.clone();
        h.click(h.key(30), egui::PointerButton::Primary);
        let s30 = h.set.note_on.keys[0].shape.clone();
        let x = 0.8;
        h.click(h.at([x, s30.eval(x)]), egui::PointerButton::Primary);
        let m = &h.set.note_on;
        assert_eq!(m.knot_count(), 4);
        assert!(m.keys.iter().all(|k| k.shape.knots.len() == 4));
        for (k0, k1) in start.keys.iter().zip(&m.keys) {
            for j in 0..=50 {
                let x = j as f32 / 50.0;
                assert!((k0.shape.eval(x) - k1.shape.eval(x)).abs() < 1e-4, "нота {}", k0.note);
            }
        }
        let added = m.clone();
        h.assert_bank_in_sync();
        // Удалить новый узел (ПКМ) — из обеих ключевых.
        let i = (1..3).find(|&i| (h.set.note_on.keys[0].shape.knots[i].x - x).abs() < 0.01).unwrap_or(2);
        let p = h.set.note_on.keys[0].shape.pt(i);
        h.click(h.at(p), egui::PointerButton::Secondary);
        assert_eq!(h.set.note_on.knot_count(), 3);
        h.key_press(egui::Key::Z, egui::Modifiers::CTRL);
        assert_eq!(h.set.note_on, added);
        h.key_press(egui::Key::Z, egui::Modifiers::CTRL);
        assert_eq!(h.set.note_on, start);
        h.key_press(egui::Key::Z, egui::Modifiers::CTRL | egui::Modifiers::SHIFT);
        assert_eq!(h.set.note_on, added);
        h.assert_bank_in_sync();
    }

    /// Драг ключевой по клавиатуре переносит её между соседями.
    #[test]
    fn drag_key_note_on_keyboard() {
        let mut h = Harness::with(two_keys());
        let out = h.drag(h.key(90), h.key(100));
        assert_eq!(h.set.note_on.keys[1].note, 100);
        assert_eq!(h.ui.note(), 100);
        assert!(out.dirty[0].is_some() && out.committed);
        h.drag(h.key(100), h.key(10 + NOTE_LO));
        assert_eq!(h.set.note_on.keys[1].note, 31, "не дальше соседней ключевой");
        h.assert_bank_in_sync();
        // Драг не по ключевой — только выбор нот.
        let m = h.set.note_on.clone();
        h.drag(h.key(50), h.key(70));
        assert_eq!((h.ui.note(), &h.set.note_on), (70, &m));
    }

    #[test]
    fn key_actions_make_and_remove_keys_and_overrides() {
        let mut h = Harness::with(two_keys());
        let start = h.set.note_on.clone();
        let mut out = Outcome::default();
        h.ui.key_action(&mut h.set, KeyAction::MakeKey(60), &mut out);
        assert_eq!(h.set.note_on.key_at(60), Some(1));
        // Ключевая → исключение: своя форма та же, морф без неё.
        let s60 = h.set.note_on.shape_at(60);
        h.ui.key_action(&mut h.set, KeyAction::MakeOverride(60), &mut out);
        assert_eq!((h.set.note_on.key_at(60), h.set.note_on.override_at(60)), (None, Some(0)));
        assert_eq!(h.set.note_on.shape_at(60), s60);
        // Исключение → ключевая со своей формой.
        h.ui.key_action(&mut h.set, KeyAction::MakeKey(60), &mut out);
        assert_eq!(h.set.note_on.override_at(60), None);
        assert_eq!(h.set.note_on.shape_at(60), s60);
        h.ui.key_action(&mut h.set, KeyAction::RemoveKey(1), &mut out);
        assert_eq!(h.set.note_on, start);
        h.ui.key_action(&mut h.set, KeyAction::MakeOverride(45), &mut out);
        h.ui.key_action(&mut h.set, KeyAction::RemoveOverride(45), &mut out);
        assert_eq!(h.set.note_on, start);
        assert_eq!(out.dirty[0], Some((30, 90)));
        // Последнюю ключевую не убрать.
        let mut one = Harness::new();
        one.ui.key_action(&mut one.set, KeyAction::RemoveKey(0), &mut out);
        assert_eq!(one.set.note_on.keys.len(), 1);
    }

    /// «Diagonal» и форма из библиотеки — в выбранную ноту; «Reset notes» —
    /// одна диагональ на всю кривую.
    #[test]
    fn diagonal_and_reset_notes() {
        let mut h = Harness::with(two_keys());
        let mut out = Outcome::default();
        h.ui.views[0].note = 90;
        let mut wavy = CurveShape::linear();
        for x in [0.2, 0.4, 0.6, 0.8] {
            let i = wavy.insert_at(x).unwrap();
            wavy.move_knot(i, x, 1.0 - x);
        }
        h.ui.set_note_shape(&mut h.set, wavy.clone(), &mut out);
        let m = &h.set.note_on;
        assert_eq!(m.knot_count(), 6, "узлы формы добавлены во всю карту");
        assert_eq!(m.keys[1].shape, wavy);
        h.ui.views[0].note = 45;
        h.ui.set_note_shape(&mut h.set, CurveShape::linear(), &mut out);
        let m = &h.set.note_on;
        assert_eq!(m.keys.iter().map(|k| k.note).collect::<Vec<_>>(), vec![30, 45, 90]);
        for j in 0..=20 {
            let x = j as f32 / 20.0;
            assert!((m.shape_at(45).eval(x) - x).abs() < 1e-5);
        }
        h.set.note_on.set_override(50, bent());
        h.ui.reset_notes(&mut h.set, &mut out);
        assert_eq!(h.set.note_on, NoteMap::default());
        out.store(&h.set, &h.bank);
        h.assert_bank_in_sync();
    }

    /// Карта с ключевыми 30, 60, 90 (60 — точная вставка), выбраны нота 60 и
    /// её средний узел: дорожка y узла 1 на поле дорожек.
    fn lane_harness() -> (Harness, NoteMap) {
        let mut h = Harness::with(two_keys());
        let mut out = Outcome::default();
        h.ui.key_action(&mut h.set, KeyAction::MakeKey(60), &mut out);
        out.store(&h.set, &h.bank);
        let p = h.set.note_on.keys[1].shape.pt(1);
        h.click(h.at(p), egui::PointerButton::Primary);
        assert_eq!((h.ui.note(), h.ui.views[0].sel), (60, Some(1)));
        let start = h.set.note_on.clone();
        (h, start)
    }

    /// Какие ноты изменились против карты `a`.
    fn changed_notes(a: &NoteMap, b: &NoteMap) -> Vec<u8> {
        (0..NOTES as u8).filter(|&n| a.shape_at(n) != b.shape_at(n)).collect()
    }

    /// ПКМ-драг ручки дорожки — только ноты своего отрезка; ЛКМ — обоих
    /// (вторая ручка зеркально); двойной клик по узлу дорожки — снова
    /// линейный морф, карта бит-в-бит; отмена.
    #[test]
    fn lane_handles_bend_only_their_segments() {
        let (mut h, start) = lane_harness();
        let m = &h.set.note_on;
        let tip = m.lane_seg(Axis::Y, 1, 1)[1];
        let out = h.drag_by(h.lane(tip), h.lane(tip) + egui::vec2(0.0, -30.0), egui::PointerButton::Secondary);
        assert_eq!(out.dirty[0], Some((30, 90)));
        assert_eq!(changed_notes(&start, &h.set.note_on), (61..90).collect::<Vec<_>>());
        assert!(!h.set.note_on.lanes[1].y[1].auto);
        h.assert_bank_in_sync();
        let one_side = h.set.note_on.clone();
        // ЛКМ — симметрично: меняются оба отрезка вокруг 60.
        let tip = h.set.note_on.lane_seg(Axis::Y, 1, 1)[1];
        h.drag(h.lane(tip), h.lane(tip) + egui::vec2(0.0, 20.0));
        let both: Vec<u8> = (31..90).filter(|&n| n != 60).collect();
        assert_eq!(changed_notes(&start, &h.set.note_on), both);
        let (hi, ho) = (h.set.note_on.lane_hin(Axis::Y, 1, 1), h.set.note_on.lane_hout(Axis::Y, 1, 1));
        assert!((hi[0] + ho[0]).abs() < 1e-6 && (hi[1] + ho[1]).abs() < 1e-6, "{hi:?} {ho:?}");
        h.assert_bank_in_sync();
        // Двойной клик по узлу дорожки (после паузы: иначе egui сочтёт клик тройным).
        h.t += 1.0;
        let k = h.lane(h.set.note_on.lane_pt(Axis::Y, 1, 1));
        h.click(k, egui::PointerButton::Primary);
        let out = h.click(k, egui::PointerButton::Primary);
        assert_eq!(h.set.note_on, start, "двойной клик не вернул линейный морф");
        assert!(out.committed);
        h.assert_bank_in_sync();
        h.key_press(egui::Key::Z, egui::Modifiers::CTRL);
        assert_ne!(h.set.note_on, start);
        h.key_press(egui::Key::Z, egui::Modifiers::CTRL);
        assert_eq!(h.set.note_on, one_side);
        h.assert_bank_in_sync();
    }

    /// Узел дорожки ходит только по вертикали: значение узла в ключевой.
    #[test]
    fn lane_knot_drag_sets_value_at_key() {
        let (mut h, start) = lane_harness();
        let p = h.set.note_on.lane_pt(Axis::Y, 1, 0);
        let lr = h.ui.views[0].lane_rect.unwrap();
        let out = h.drag(h.lane(p), h.lane(p) + egui::vec2(25.0, -0.2 * lr.height()));
        let k = h.set.note_on.keys[0].shape.knots[1];
        let k0 = start.keys[0].shape.knots[1];
        assert!((k.y - (k0.y + 0.2)).abs() < 0.005 && k.x == k0.x, "{k0:?} → {k:?}");
        assert_eq!(h.set.note_on.keys[0].note, 30, "узел дорожки не переносит ключевую");
        assert_eq!(h.ui.note(), 30, "драг узла выбирает его ключевую");
        assert_eq!(out.dirty[0], Some((0, 60)));
        h.assert_bank_in_sync();
        // Режим x: тот же жест двигает узел по x.
        h.ui.lane_axis = Axis::X;
        h.frame(vec![], egui::Modifiers::NONE);
        let p = h.set.note_on.lane_pt(Axis::X, 1, 0);
        h.drag(h.lane(p), h.lane(p) + egui::vec2(0.0, -0.1 * lr.height()));
        let k1 = h.set.note_on.keys[0].shape.knots[1];
        assert!((k1.x - (k.x + 0.1)).abs() < 0.005 && k1.y == k.y, "{k:?} → {k1:?}");
    }

    /// Изогнутая дорожка x узла 1 заходит за дорожку узла 2 — там узел
    /// зажат (красные точки), формы нот остаются функцией velocity.
    #[test]
    fn crossing_x_lanes_are_detected() {
        let mut m = two_keys().note_on;
        m.insert_knot(1, 0.5);
        assert!(x_clamped(&m).iter().flatten().all(Option::is_none));
        m.set_lane_handle(Axis::X, 1, 0, true, [0.2, 0.6], false);
        let cl = x_clamped(&m);
        let hits: Vec<(usize, u8)> = cl
            .iter()
            .enumerate()
            .flat_map(|(i, row)| row.iter().enumerate().filter(|(_, x)| x.is_some()).map(move |(j, _)| (i, NOTE_LO + j as u8)))
            .collect();
        assert!(!hits.is_empty() && hits.iter().all(|&(i, n)| i >= 1 && (31..90).contains(&n)), "{hits:?}");
        for n in 0..NOTES as u8 {
            let k = m.shape_at(n).knots;
            assert!(k.windows(2).all(|w| w[1].x > w[0].x), "нота {n}");
        }
    }

    /// Время кадра при драге узла ключевой ноты в карте из 8 ключевых по 6
    /// узлов (кадр egui + перезаливка таблиц изменённых нот), release:
    ///   cargo test --release --lib bench_drag_frame -- --ignored --nocapture
    #[test]
    #[ignore]
    fn bench_drag_frame_8_keys() {
        let mut set = CurveSet::default();
        let m = &mut set.note_on;
        m.keys[0].note = 21;
        for x in [0.2, 0.4, 0.6, 0.8] {
            let (seg, t) = m.keys[0].shape.locate(x);
            m.insert_knot(seg, t);
        }
        for (j, n) in [33u8, 45, 57, 69, 81, 93, 108].into_iter().enumerate() {
            let k = m.add_key(n);
            for i in 1..5 {
                let kn = m.keys[k].shape.knots[i];
                m.keys[k].shape.move_knot(i, kn.x, (kn.y + 0.05 * (j as f32 + i as f32)).min(1.0));
            }
        }
        let mut h = Harness::with(set);
        h.click(h.key(57), egui::PointerButton::Primary);
        let p = h.set.note_on.keys[3].shape.pt(2);
        let from = h.at(p);
        h.frame(vec![egui::Event::PointerMoved(from)], egui::Modifiers::NONE);
        h.button(from, egui::PointerButton::Primary, true);
        let frames = 300;
        let (mut total, mut store) = (0.0f64, 0.0f64);
        for j in 0..frames {
            let to = from + egui::vec2(0.0, -40.0 * ((j as f32 * 0.1).sin()));
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 800.0))),
                events: vec![egui::Event::PointerMoved(to)],
                time: Some(h.t),
                ..Default::default()
            };
            h.t += 1.0 / 60.0;
            let t0 = std::time::Instant::now();
            let mut out = Outcome::default();
            let _ = h.ctx.run(raw, |ctx| out = h.ui.show(ctx, &mut h.set, &h.bank, |_| {}));
            let t1 = std::time::Instant::now();
            out.store(&h.set, &h.bank);
            let t2 = std::time::Instant::now();
            assert!(j < 2 || out.dirty[0].is_some(), "кадр {j}: драг ничего не изменил");
            total += (t2 - t0).as_secs_f64();
            store += (t2 - t1).as_secs_f64();
        }
        println!(
            "кадр драга, 8 ключевых × 6 узлов: {:.2} мс (из них заливка таблиц {:.2} мс)",
            total / frames as f64 * 1e3,
            store / frames as f64 * 1e3
        );
        h.assert_bank_in_sync();
    }

    /// Пресет «всё сразу» заменяет обе кривые и формат выхода; форма из
    /// библиотеки — только выбранную ноту открытой кривой.
    #[test]
    fn presets_from_library() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::new(Some(dir.path().to_path_buf()));
        let mut saved = two_keys();
        saved.hi_res = true;
        saved.note_off.set_override(70, bent());
        lib.save_set("All", &saved).unwrap();
        lib.save("Bent", &bent()).unwrap();
        let mut h = Harness::new();
        h.ui = CurvesUi::new(&h.set, Slot::NoteOn, lib);
        let mut out = Outcome::default();
        h.ui.load_preset(&mut h.set, "All".into(), &mut out);
        assert_eq!(h.set, saved);
        assert_eq!(out.dirty, [Some((0, LAST_NOTE)); 2]);
        out.store(&h.set, &h.bank);
        assert!(h.bank.hi_res());
        h.assert_bank_in_sync();
        // Форма — в выбранную ноту 60 Note Off (промежуточная → ключевая).
        h.ui.tab = Slot::NoteOff;
        let before = h.set.note_on.clone();
        let mut out = Outcome::default();
        h.ui.load_preset(&mut h.set, "Bent".into(), &mut out);
        assert_eq!(h.set.note_on, before);
        let m = &h.set.note_off;
        assert_eq!(m.shape_at(60), bent());
        assert_eq!(m.shape_at(70), bent(), "исключение 70 не тронуто");
        assert!(out.dirty[0].is_none() && out.dirty[1].is_some());
        // Битое имя — сообщение, без изменений.
        let set = h.set.clone();
        h.ui.load_preset(&mut h.set, "nope".into(), &mut out);
        assert_eq!(h.set, set);
        assert!(h.ui.notice.as_ref().is_some_and(|(err, _)| *err));
    }
}
