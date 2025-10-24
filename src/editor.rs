//! GUI редактор для плагина MIDI Curves

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, EguiState};
use std::sync::Arc;

use crate::curve::BezierCurve;
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
                
                // Клонируем кривую, чтобы не держать lock
                let curve = state.lock().unwrap().curve().clone();
                
                let mut curve_canvas = CurveCanvas::new(&curve);
                curve_canvas.ui(ui);
            });
        },
    )
}

/// Canvas для отрисовки графика кривой
struct CurveCanvas<'a> {
    curve: &'a BezierCurve,
}

impl<'a> CurveCanvas<'a> {
    fn new(curve: &'a BezierCurve) -> Self {
        Self { curve }
    }

    fn ui(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let (response, painter) = ui.allocate_painter(
            egui::Vec2::new(600.0, 400.0),
            egui::Sense::click_and_drag(),
        );

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
        
        // Отрисовка контрольных точек
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
        for point in self.curve.control_points() {
            let screen_pos = self.world_to_screen(point.position, rect);
            
            // Внешний круг
            painter.circle_filled(
                screen_pos,
                10.0,
                egui::Color32::from_rgb(255, 100, 100),
            );
            
            // Внутренний круг
            painter.circle_filled(
                screen_pos,
                6.0,
                egui::Color32::from_rgb(255, 150, 150),
            );
        }
    }

    fn world_to_screen(&self, world_pos: kurbo::Point, rect: egui::Rect) -> egui::Pos2 {
        egui::pos2(
            rect.left() + (world_pos.x as f32 / 127.0) * rect.width(),
            rect.bottom() - (world_pos.y as f32 / 127.0) * rect.height(),
        )
    }

    #[allow(dead_code)]
    fn screen_to_world(&self, screen_pos: egui::Pos2, rect: egui::Rect) -> kurbo::Point {
        kurbo::Point::new(
            ((screen_pos.x - rect.left()) / rect.width() * 127.0) as f64,
            ((rect.bottom() - screen_pos.y) / rect.height() * 127.0) as f64,
        )
    }
}