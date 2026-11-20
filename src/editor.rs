//! Редактор кривых velocity (egui) — общий для плагина и standalone. Жесты и
//! отрисовка — из редактора piano_sim (`src/gui/vel_curve_editor.rs`).
//!
//! Мышь на графике: ЛКМ по кривой — новый узел (форма не меняется), драг узла
//! или ручки — изгиб, ПКМ или двойной клик по узлу — удалить его, Delete —
//! удалить выбранный. Shift — точный ход (×0.1), Ctrl — привязка к сетке 1/16,
//! Alt+драг ручки — сломать узел в угол, Alt+клик по узлу — гладкий ↔ угол.
//! Ctrl+Z / Ctrl+Shift+Z (Ctrl+Y) — отмена / повтор, история у каждой кривой
//! своя; снимок формы — на отпускании мыши.
//!
//! Экономия CPU: ломаная кривой (257 точек) пересчитывается только при
//! изменении формы; таблицу потока MIDI перезаливает вызывающий — только по
//! флагам [`Outcome`].

use crate::curve::{CurveBank, CurveSet, CurveShape, Slot};
use crate::egui::{self, Color32, Pos2, Rect, Stroke};
use crate::library::{self, Entry, Library};

/// Радиусы захвата узла и ручки, пиксели.
const HIT_KNOT: f32 = 8.0;
const HIT_HANDLE: f32 = 7.0;
/// Шаг привязки (Ctrl) в единичном квадрате.
const SNAP: f32 = 1.0 / 16.0;
/// Отрезков ломаной кривой на графике.
const POLY_SEGS: usize = 256;
/// Глубина истории отмены каждой кривой.
const HISTORY_DEPTH: usize = 100;

const COL_NOTE_ON: Color32 = Color32::from_rgb(100, 200, 255);
const COL_NOTE_OFF: Color32 = Color32::from_rgb(255, 150, 100);
const COL_MARKER: Color32 = Color32::from_rgb(120, 230, 140);

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

#[derive(Clone, Copy, PartialEq, Debug)]
enum Grab {
    Knot(usize),
    In(usize),
    Out(usize),
}

/// Вид и жест одной кривой.
#[derive(Default)]
struct CurveView {
    /// Выбранный узел (его ручки видны и хватаются).
    sel: Option<usize>,
    grab: Option<Grab>,
    /// Позиция захваченного объекта БЕЗ клампа: шаг мыши копится сюда, чтобы
    /// упор в край или соседний узел не «прилипал» при обратном ходе.
    grab_pos: [f32; 2],
    /// Ломаная в единичном квадрате и форма, по которой она посчитана.
    poly: Vec<[f32; 2]>,
    poly_of: Option<CurveShape>,
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

/// История формы одной кривой: снимки на отпускании мыши.
#[derive(Clone, Debug)]
pub struct History {
    undo: Vec<CurveShape>,
    redo: Vec<CurveShape>,
    /// Последняя зафиксированная форма.
    committed: CurveShape,
}

impl History {
    pub fn new(shape: &CurveShape) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), committed: shape.clone() }
    }

    /// Зафиксировать текущую форму; `true` — она отличалась от прежней.
    pub fn commit(&mut self, cur: &CurveShape) -> bool {
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
    pub fn undo(&mut self, cur: &mut CurveShape) -> bool {
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

    pub fn redo(&mut self, cur: &mut CurveShape) -> bool {
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
    /// Форма кривой изменилась (индекс — [`Slot::idx`]): перезалить таблицу.
    pub shape: [bool; 2],
    /// Переключён формат выхода.
    pub hi_res: bool,
    /// Переключена вкладка.
    pub tab: bool,
    /// Ввод отпущен и форма зафиксирована в истории — момент сохранить.
    pub committed: bool,
}

impl Outcome {
    pub fn any(&self) -> bool {
        self.shape[0] || self.shape[1] || self.hi_res || self.tab || self.committed
    }
}

/// Окно редактора: вкладки Note On / Note Off, график, библиотека форм.
pub struct CurvesUi {
    pub tab: Slot,
    views: [CurveView; 2],
    hist: [History; 2],
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
        }
    }

    /// Перечитать папку библиотеки при следующем показе.
    pub fn rescan_library(&mut self) {
        self.lib_list = None;
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
        let (undo_key, redo_key) = undo_keys(ctx);
        let mut undo_req = undo_key;
        let mut redo_req = redo_key;

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
                if ui.button("Diagonal").on_hover_text("Reset this curve to y = x").clicked() {
                    *set.shape_mut(self.tab) = CurveShape::linear();
                    self.views[self.tab.idx()].sel = None;
                    out.shape[self.tab.idx()] = true;
                }
            });
        });

        egui::SidePanel::right("curves_side").resizable(false).exact_width(250.0).show(ctx, |ui| {
            side(ui);
            self.library_panel(ui, set, &mut out);
            ui.separator();
            let tab = self.tab;
            match bank.last_x(tab) {
                Some(x) => {
                    let y = set.shape(tab).eval(x);
                    ui.label(format!("Last {}: {} → {}", slot_name(tab), fmt_vel(x), fmt_vel(y)));
                }
                None => {
                    ui.label(format!("Last {}: —", slot_name(tab)));
                }
            }
            ui.label(egui::RichText::new(format!("Notes processed: {}", bank.notes())).weak());
            ui.add_space(6.0);
            ui.label(egui::RichText::new(HELP).small().weak());
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let tab = self.tab;
            let marker = bank.last_x(tab);
            let view = &mut self.views[tab.idx()];
            if curve_editor(ui, set.shape_mut(tab), view, color(tab), marker) {
                out.shape[tab.idx()] = true;
            }
        });

        // История: снимок на отпускании ввода, отмена — только вне жеста.
        let released = input_released(ctx, &mut self.prev_busy);
        let tab = self.tab.idx();
        if !self.prev_busy && (undo_req || redo_req) {
            let shape = set.shape_mut(self.tab);
            let done = if undo_req { self.hist[tab].undo(shape) } else { self.hist[tab].redo(shape) };
            if done {
                self.views[tab].sel = None;
                out.shape[tab] = true;
                out.committed = true;
            }
        }
        if released {
            for slot in Slot::ALL {
                out.committed |= self.hist[slot.idx()].commit(set.shape(slot));
            }
        }
        out
    }

    fn library_panel(&mut self, ui: &mut egui::Ui, set: &mut CurveSet, out: &mut Outcome) {
        ui.label(egui::RichText::new("Presets").strong());
        let list = self.lib_list.get_or_insert_with(|| self.lib.scan()).clone();
        let mut load: Option<String> = None;
        egui::ScrollArea::vertical().max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
            for e in &list {
                let text = if e.file { egui::RichText::new(&e.name) } else { egui::RichText::new(&e.name).italics() };
                let r = ui.selectable_label(self.lib_name == e.name, text);
                if r.clicked() {
                    load = Some(e.name.clone());
                }
            }
        });
        if let Some(name) = load {
            match self.lib.load(&name) {
                Ok(shape) => {
                    *set.shape_mut(self.tab) = shape;
                    self.views[self.tab.idx()].sel = None;
                    out.shape[self.tab.idx()] = true;
                    self.notice = Some((false, format!("Loaded «{name}» into {}", slot_name(self.tab))));
                    self.lib_name = name;
                }
                Err(e) => self.notice = Some((true, format!("«{name}»: {e}"))),
            }
            self.delete_armed = None;
        }
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.lib_name).desired_width(120.0).hint_text("name"));
            let name = self.lib_name.trim().to_string();
            let is_file = list.iter().any(|e| e.file && e.name == name);
            let label = if is_file { "Overwrite" } else { "Save" };
            if ui
                .add_enabled(library::name_ok(&name), egui::Button::new(label))
                .on_hover_text("Save the current curve to the preset folder")
                .clicked()
            {
                match self.lib.save(&name, set.shape(self.tab)) {
                    Ok(()) => {
                        self.notice = Some((false, format!("Saved «{name}»")));
                        self.lib_list = None;
                    }
                    Err(e) => self.notice = Some((true, format!("«{name}»: {e}"))),
                }
                self.delete_armed = None;
            }
        });
        ui.horizontal(|ui| {
            let name = self.lib_name.trim().to_string();
            let is_file = list.iter().any(|e| e.file && e.name == name);
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
                    self.lib_list = None;
                    self.delete_armed = None;
                } else {
                    self.delete_armed = Some(name);
                }
            }
            if ui.button("Rescan").on_hover_text("Re-read the preset folder").clicked() {
                self.lib_list = None;
            }
        });
        if let Some((err, text)) = &self.notice {
            let c = if *err { Color32::from_rgb(230, 110, 100) } else { Color32::from_gray(160) };
            ui.colored_label(c, text);
        }
    }
}

const HELP: &str = "Click on the curve — add a knot. Drag a knot or handle. \
Right/double click a knot — delete; Del — delete selected. \
Shift — fine (×0.1), Ctrl — snap to 1/16, Alt+drag handle — corner, \
Alt+click knot — smooth ↔ corner. Ctrl+Z / Ctrl+Shift+Z — undo / redo. \
Italic presets are built in.";

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

/// График одной кривой с жестами. `true` — форма изменилась.
fn curve_editor(
    ui: &mut egui::Ui,
    shape: &mut CurveShape,
    st: &mut CurveView,
    col: Color32,
    marker: Option<f32>,
) -> bool {
    let mut changed = false;
    let mods = ui.input(|i| i.modifiers);
    let snap = mods.ctrl || mods.command;
    let avail = ui.available_size();
    let (resp, painter) = ui.allocate_painter(avail, egui::Sense::click_and_drag());
    let pr = Rect::from_min_max(resp.rect.min + egui::vec2(40.0, 10.0), resp.rect.max - egui::vec2(12.0, 26.0));
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

    // Удаление: ПКМ / двойной клик по узлу, Delete — выбранный.
    let mut removed = false;
    if resp.secondary_clicked() || resp.double_clicked() {
        if let Some(Grab::Knot(i)) = resp.interact_pointer_pos().and_then(|p| hit(shape, None, p)) {
            if i > 0 && i + 1 < shape.knots.len() {
                shape.remove(i);
                st.sel = None;
                changed = true;
            }
            removed = true;
        }
    }
    let del = !ui.ctx().wants_keyboard_input()
        && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
    if let (true, Some(i)) = (del, st.sel) {
        if i > 0 && i + 1 < shape.knots.len() {
            shape.remove(i);
            st.sel = None;
            changed = true;
        }
    }
    // Клик: выбрать узел / Alt — гладкий↔угол / по кривой — новый узел.
    if resp.clicked() && !removed {
        if let Some(p) = resp.interact_pointer_pos() {
            match hit(shape, st.sel, p) {
                Some(Grab::Knot(i)) if mods.alt => {
                    shape.toggle_smooth(i);
                    st.sel = Some(i);
                    changed = true;
                }
                Some(Grab::Knot(i)) => st.sel = Some(i),
                Some(_) => {}
                None if near_curve(shape, p) => {
                    if let Some(i) = shape.insert_at(to_u(p)[0]) {
                        st.sel = Some(i);
                        changed = true;
                    }
                }
                None => st.sel = None,
            }
        }
    }
    // Драг: захват по точке НАЖАТИЯ (drag_started приходит после порога).
    if resp.drag_started_by(egui::PointerButton::Primary) {
        let origin = ui.input(|i| i.pointer.press_origin());
        st.grab = origin.and_then(|p| match hit(shape, st.sel, p) {
            Some(g) => Some(g),
            None if near_curve(shape, p) => shape.insert_at(to_u(p)[0]).map(|i| {
                changed = true;
                Grab::Knot(i)
            }),
            None => None,
        });
        if let Some(g) = st.grab {
            st.grab_pos = grab_target_pos(shape, g);
            if let Grab::Knot(i) = g {
                st.sel = Some(i);
            }
        }
    }
    if let (true, Some(g)) = (resp.dragged_by(egui::PointerButton::Primary), st.grab) {
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
                Grab::Out(i) => shape.set_handle(i, true, p, mods.alt),
                Grab::In(i) => shape.set_handle(i, false, p, mods.alt),
            }
            changed |= *shape != before;
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
    let pts: Vec<Pos2> = st.poly(shape).iter().map(|&u| to_s(u)).collect();
    painter.add(egui::Shape::line(pts, Stroke::new(2.0_f32, col)));
    // Velocity последней ноты: где на кривой сейчас играли.
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
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
