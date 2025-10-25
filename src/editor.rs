//! GUI редактор для плагина MIDI Curves 
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, EguiState};
use std::sync::Arc;

use crate::curve::BezierCurve;
use crate::gui::InteractionHandler;
use crate::processor::VelocityCurveProcessor;

/// Создание GUI редактора
pub fn create_editor(
    curve_processor: Arc<std::sync::Mutex<VelocityCurveProcessor>>,
) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        EguiState::from_size(800, 600),
        curve_processor.clone(),
        |_, _| {},
        move |egui_ctx, _setter, state| {
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                ui.heading("MIDI Velocity Curves");
                
                ui.add_space(10.0);
                
                // Забираем lock на процессор для чтения и записи
                let mut curve_processor_guard = state.lock().unwrap();
                let curve = curve_processor_guard.curve().clone();
                
                // Создаем интерактивный canvas
                let mut curve_canvas = CurveCanvas::new(curve);
                let _response = curve_canvas.ui(ui);
                
                // Если кривая изменилась в canvas, обновляем ее в процессоре
                if curve_canvas.curve_changed() {
                    curve_processor_guard.set_curve(curve_canvas.curve().clone());
                }
            });
        },
    )
}

/// Canvas для отрисовки и редактирования графика кривой
struct CurveCanvas {
    curve: BezierCurve,
    interaction_handler: InteractionHandler,
    curve_modified: bool,
}

impl CurveCanvas {
    fn new(curve: BezierCurve) -> Self {
        Self {
            curve,
            interaction_handler: InteractionHandler::new(),
            curve_modified: false,
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let (response, painter) = ui.allocate_painter(
            egui::Vec2::new(600.0, 400.0),
            egui::Sense::click_and_drag(),
        );

        // Обработка взаимодействий
        let curve_changed = self.interaction_handler.handle_input(
            &response,
            &mut self.curve,
            response.rect,
        );
            
        if curve_changed {
            self.curve_modified = true;
        }

        // Отрисовка фона
        painter.rect_filled(
            response.rect,
            0.0,
            egui::Color32::from_gray(25),
        );

        // Отрисовка сетки
        self.draw_grid(&painter, response.rect);
        
        // Отрисовка кривой
        self.draw_curve(&painter, response.rect);
        
        // Отрисовка контрольных точек с учетом выбора и наведения
        self.draw_control_points(&painter, response.rect);

        response
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: egui::Rect) {
        let grid_color = egui::Color32::from_gray(40);
        
        // Вертикальные линии
        for i in 0..=10 {
            let x = rect.left() + (rect.width() * i as f32 / 10.0);
            
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1.0, grid_color),
            );
        }
        
        // Горизонтальные линии
        for i in 0..=10 {
            let y = rect.top() + (rect.height() * i as f32 / 10.0);
            
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(1.0, grid_color),
            );
        }
    }

    fn draw_curve(&self, painter: &egui::Painter, rect: egui::Rect) {
        // Отрисовка кривой Безье
        let mut points = Vec::new();
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            let x = rect.left() + t * rect.width();
            let y_norm = self.curve.evaluate(t);
            let y = rect.bottom() - y_norm * rect.height();
            points.push(egui::pos2(x, y));
        }

        painter.add(egui::Shape::line(
            points,
            egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 200, 255)),
        ));
    }

    fn draw_control_points(&self, painter: &egui::Painter, rect: egui::Rect) {
        // Отрисовка управляющих точек
        for (i, point) in self.curve.control_points().iter().enumerate() {
            let screen_pos = self.world_to_screen(point.position, rect);
            
            // Определяем цвет точки в зависимости от состояния
            let (outer_color, inner_color) = if Some(i) == self.interaction_handler.selected_point() {
                (egui::Color32::from_rgb(255, 255, 100), egui::Color32::from_rgb(255, 200, 50)) // Выбранная точка
            } else if Some(i) == self.interaction_handler.hovered_point() {
                (egui::Color32::from_rgb(255, 150, 100), egui::Color32::from_rgb(255, 100, 50)) // Точка под курсором
            } else {
                (egui::Color32::from_rgb(255, 100, 100), egui::Color32::from_rgb(255, 150, 150)) // Обычная точка
            };
            
            // Отрисовка касательных линий для выбранной точки
            if Some(i) == self.interaction_handler.selected_point() {
                // Входящая касательная
                let handle_in_screen = self.world_to_screen(
                    kurbo::Point::new(
                        point.position.x + point.handle_in.x,
                        point.position.y + point.handle_in.y,
                    ),
                    rect,
                );
                painter.line_segment(
                    [screen_pos, handle_in_screen],
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 200, 100)),
                );
                
                // Исходящая касательная
                let handle_out_screen = self.world_to_screen(
                    kurbo::Point::new(
                        point.position.x + point.handle_out.x,
                        point.position.y + point.handle_out.y,
                    ),
                    rect,
                );
                painter.line_segment(
                    [screen_pos, handle_out_screen],
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 100, 200)),
                );
                
                // Отрисовка точек касательных
                painter.circle_filled(
                    handle_in_screen,
                    4.0,
                    egui::Color32::from_rgb(100, 200, 100),
                );
                painter.circle_filled(
                    handle_out_screen,
                    4.0,
                    egui::Color32::from_rgb(100, 100, 200),
                );
            }
            
            // Внешний круг
            painter.circle_filled(
                screen_pos,
                10.0,
                outer_color,
            );
            
            // Внутренний круг
            painter.circle_filled(
                screen_pos,
                6.0,
                inner_color,
            );
        }
    }

    fn world_to_screen(&self, world_pos: kurbo::Point, rect: egui::Rect) -> egui::Pos2 {
        egui::pos2(
            rect.left() + (world_pos.x as f32 / 127.0) * rect.width(),
            rect.bottom() - (world_pos.y as f32 / 127.0) * rect.height(),
        )
    }

    fn screen_to_world(&self, screen_pos: egui::Pos2, rect: egui::Rect) -> kurbo::Point {
        kurbo::Point::new(
            ((screen_pos.x - rect.left()) / rect.width() * 127.0) as f64,
            ((rect.bottom() - screen_pos.y) / rect.height() * 127.0) as f64,
        )
    }

    /// Возвращает ссылку на кривую
    pub fn curve(&self) -> &BezierCurve {
        &self.curve
    }

    /// Проверяет, была ли кривая изменена
    pub fn curve_changed(&self) -> bool {
        self.curve_modified
    }
}