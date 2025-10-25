//! Обработчик взаимодействий для интерактивного редактирования кривой

use nih_plug_egui::egui::{Pos2, Response, Rect};
use kurbo::Point;

use crate::curve::BezierCurve;

/// Состояние перетаскивания
#[derive(Debug, Clone)]
pub struct DragState {
    pub is_dragging: bool,
    pub drag_start: Pos2,
    pub dragged_point_index: Option<usize>,
}

impl Default for DragState {
    fn default() -> Self {
        Self {
            is_dragging: false,
            drag_start: Pos2::new(0.0, 0.0),
            dragged_point_index: None,
        }
    }
}

/// Обработчик взаимодействий для кривой
pub struct InteractionHandler {
    pub selected_point: Option<usize>,
    pub drag_state: DragState,
    pub hovered_point: Option<usize>,
}

impl Default for InteractionHandler {
    fn default() -> Self {
        Self {
            selected_point: None,
            drag_state: DragState::default(),
            hovered_point: None,
        }
    }
}

impl InteractionHandler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Обработка ввода для кривой
    pub fn handle_input(
        &mut self,
        response: &Response,
        curve: &mut BezierCurve,
        rect: Rect,
    ) -> bool {
        let mut changed = false;
        
        // Получаем позицию курсора в координатах экрана
        if let Some(hover_pos) = response.hover_pos() {
            // Проверяем наведение на точки
            self.hovered_point = self.find_point_at(hover_pos, curve, rect) {
        } else {
            self.hovered_point = None;
        }

        // Обработка кликов для выбора точки
        if response.clicked() {
            if let Some(hover_pos) = response.hover_pos() {
                self.selected_point = self.find_point_at(hover_pos, curve, rect);
                
                // Начинаем перетаскивание если кликнули на точку
                if self.selected_point.is_some() {
                    self.drag_state.is_dragging = true;
                    self.drag_state.drag_start = hover_pos;
                    self.drag_state.dragged_point_index = self.selected_point;
                }
            }
        }

        // Перетаскивание точки
        if response.dragged() {
            if let Some(selected) = self.selected_point {
                if let Some(hover_pos) = response.hover_pos() {
                    let world_pos = self.screen_to_world(hover_pos, rect);
                    curve.update_control_point(selected, world_pos);
                    changed = true;
                }
            }
        }

        // Завершение перетаскивания
        if response.drag_released() {
            self.drag_state.is_dragging = false;
            self.drag_state.dragged_point_index = None;
        }

        // Двойной клик для добавления точки
        if response.double_clicked() {
            if let Some(hover_pos) = response.hover_pos() {
                let world_pos = self.screen_to_world(hover_pos, rect);
                curve.add_control_point(world_pos);
                self.selected_point = Some(curve.control_points().len() - 2); // Выбираем новую точку (не последнюю)
                changed = true;
            }
        }

        // Правый клик для удаления точки
        if response.secondary_clicked() {
            if let Some(hover_pos) = response.hover_pos() {
                if let Some(point_idx) = self.find_point_at(hover_pos, curve, rect) {
                    if point_idx > 0 && point_idx < curve.control_points().len() - 1 {
                        curve.remove_control_point(point_idx);
                        self.selected_point = None;
                        changed = true;
                    }
                }
            }
        }

        changed
    }

    /// Поиск точки в радиусе клика
    fn find_point_at(
        &self,
        screen_pos: Pos2,
        curve: &BezierCurve,
        rect: Rect,
    ) -> Option<usize> {
        const CLICK_RADIUS: f32 = 12.0;
        
        for (i, point) in curve.control_points().iter().enumerate() {
            let point_screen = self.world_to_screen(point.position, rect);
            if screen_pos.distance(point_screen) < CLICK_RADIUS {
                return Some(i);
            }
        }
        None
    }

    /// Преобразование координат мира в экранные
    fn world_to_screen(&self, world_pos: Point, rect: Rect) -> Pos2 {
        Pos2::new(
            rect.left() + (world_pos.x as f32 / 127.0) * rect.width(),
            rect.bottom() - (world_pos.y as f32 / 127.0) * rect.height(),
        )
    }

    /// Преобразование координат экранных в мировые
    fn screen_to_world(&self, screen_pos: Pos2, rect: Rect) -> Point {
        Point::new(
            ((screen_pos.x - rect.left()) / rect.width() * 127.0) as f64,
            ((rect.bottom() - screen_pos.y) / rect.height() * 127.0) as f64,
        )
    }

    /// Получение индекса выбранной точки
    pub fn selected_point(&self) -> Option<usize> {
        self.selected_point
    }

    /// Получение индекса точки под курсором
    pub fn hovered_point(&self) -> Option<usize> {
        self.hovered_point
    }

    /// Сброс состояния выбора
    pub fn reset_selection(&mut self) {
        self.selected_point = None;
        self.hovered_point = None;
        self.drag_state.is_dragging = false;
        self.drag_state.dragged_point_index = None;
    }
}