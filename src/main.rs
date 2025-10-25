//! MIDI Curves - Standalone приложение для обработки MIDI velocity с настраиваемыми кривыми
//! 
//! Это standalone версия плагина, которая предоставляет полноценный GUI интерфейс
//! для редактирования кривых Безье и обработки MIDI в реальном времени.

use eframe::egui;
use std::sync::{Arc, Mutex};

// Подключаем модули проекта
mod curve;
mod presets;

use curve::{BezierCurve, ControlPoint};
use presets::{PresetManager, CurvePreset};
use egui::{Pos2, Rect, Sense, Response, Painter, Color32, Stroke, Shape};

// MIDI порт структура
#[derive(Debug, Clone)]
struct MidiPort {
    name: String,
    is_input: bool,
    is_output: bool,
}

// Состояние MIDI
struct MidiState {
    input_ports: Vec<MidiPort>,
    output_ports: Vec<MidiPort>,
    selected_input: Option<usize>,
    selected_output: Option<usize>,
    input_velocity: u8,
    output_velocity: u8,
}

// Главная структура приложения
struct MidiCurvesApp {
    // Ядро обработки кривых
    curve: Arc<Mutex<BezierCurve>>,
    
    // Состояние MIDI
    midi_state: MidiState,
    
    // Состояние интерфейса
    selected_point: Option<usize>,
    is_dragging: bool,
    drag_start: Option<Pos2>,
    
    // Менеджер пресетов
    preset_manager: Arc<Mutex<PresetManager>>,
    selected_preset: Option<String>,
}

// Встроенные пресеты кривых
impl MidiCurvesApp {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let preset_manager = Arc::new(Mutex::new(PresetManager::new()?));
        
        // Создаем встроенные пресеты если их нет
        {
            let mut manager = preset_manager.lock().unwrap();
            manager.create_builtin_presets()?;
        }
        
        Ok(Self {
            curve: Arc::new(Mutex::new(BezierCurve::new())),
            midi_state: MidiState {
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                selected_input: None,
                selected_output: None,
                input_velocity: 64,
                output_velocity: 64,
            },
            selected_point: None,
            is_dragging: false,
            drag_start: None,
            preset_manager,
            selected_preset: None,
        })
    }
    
    // Обработка MIDI входного сигнала
    fn process_input_velocity(&self, input_velocity: u8) -> u8 {
        let mut curve = self.curve.lock().unwrap();
        curve.evaluate(input_velocity as f32) as u8
    }
    
    // Тестирование кривой
    fn test_curve(&mut self) {
        self.midi_state.output_velocity = self.process_input_velocity(self.midi_state.input_velocity);
    }
    
    // Загрузка пресета
    fn load_preset(&mut self, preset_name: &str) {
        if let Some(preset) = self.preset_manager.lock().unwrap().get_preset(preset_name) {
            let mut curve = self.curve.lock().unwrap();
            curve.control_points = preset.to_control_points();
            curve.dirty = true;
            self.selected_preset = Some(preset_name.to_string());
        }
    }
    
    // Сохранение текущей кривой как пресета
    fn save_current_as_preset(&mut self, name: String, description: String) -> Result<(), Box<dyn std::error::Error>> {
        let curve = self.curve.lock().unwrap();
        let points = curve.control_points.clone();
        
        let preset = CurvePreset::new(name, description, points);
        self.preset_manager.lock().unwrap().add_preset(preset)?;
        
        Ok(())
    }
    
    // Сброс к линейной кривой
    fn reset_curve(&mut self) {
        let mut curve = self.curve.lock().unwrap();
        curve.reset_to_linear();
        self.selected_preset = None;
        self.selected_point = None;
    }
    
    // Добавление контрольной точки
    fn add_control_point(&mut self, position: Pos2, rect: Rect) {
        let world_pos = self.screen_to_world(position, rect);
        let mut curve = self.curve.lock().unwrap();
        curve.add_control_point((world_pos.x, world_pos.y));
    }
    
    // Удаление выбранной точки
    fn remove_selected_point(&mut self) -> bool {
        if let Some(index) = self.selected_point {
            let mut curve = self.curve.lock().unwrap();
            let removed = curve.remove_control_point(index);
            if removed {
                self.selected_point = None;
            }
            removed
        } else {
            false
        }
    }
    
    // Обновление позиции точки
    fn update_selected_point(&mut self, position: Pos2, rect: Rect) {
        if let Some(index) = self.selected_point {
            let world_pos = self.screen_to_world(position, rect);
            let mut curve = self.curve.lock().unwrap();
            curve.update_control_point(index, (world_pos.x, world_pos.y));
        }
    }
    
    // Конвертация экранных координат в мировые
    fn screen_to_world(&self, screen_pos: Pos2, rect: Rect) -> Pos2 {
        let x = (screen_pos.x - rect.left()) / rect.width() * 127.0;
        let y = (rect.bottom() - screen_pos.y) / rect.height() * 127.0;
        Pos2::new(x, y)
    }
    
    // Конвертация мировых координат в экранные
    fn world_to_screen(&self, world_pos: Pos2, rect: Rect) -> Pos2 {
        let x = rect.left() + world_pos.x / 127.0 * rect.width();
        let y = rect.bottom() - world_pos.y / 127.0 * rect.height();
        Pos2::new(x, y)
    }
    
    // Поиск точки под курсором
    fn find_point_at(&self, screen_pos: Pos2, rect: Rect) -> Option<usize> {
        const CLICK_RADIUS: f32 = 12.0;
        let curve = self.curve.lock().unwrap();
        
        for (i, point) in curve.control_points.iter().enumerate() {
            let point_screen = self.world_to_screen(Pos2::new(point.position.0, point.position.1), rect);
            if screen_pos.distance(point_screen) < CLICK_RADIUS {
                return Some(i);
            }
        }
        None
    }
}

impl eframe::App for MidiCurvesApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Отрисовка главного окна
        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_main_ui(ui);
        });
        
        // Перерисовка при изменении состояния
        ctx.request_repaint();
    }
}

impl MidiCurvesApp {
    fn draw_main_ui(&mut self, ui: &mut egui::Ui) {
        // Заголовок
        ui.horizontal_top(|ui| {
            ui.heading("🎹 MIDI Curves - Standalone");
            ui.separator();
            ui.label("Редактор кривых для обработки MIDI velocity");
        });
        
        ui.separator();
        
        // Основная область с кривой и контролами
        ui.horizontal(|ui| {
            // Левая панель - Кривая и тестирование
            ui.vertical(|ui| {
                self.draw_curve_editor(ui);
                self.draw_test_panel(ui);
            });
            
            ui.add_space(20.0);
            
            // Правая панель - Пресеты и настройки
            ui.vertical(|ui| {
                self.draw_presets_panel(ui);
                self.draw_midi_panel(ui);
            });
        });
    }
    
    fn draw_curve_editor(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label("🎯 Редактор кривой");
            
            // Область для отрисовки кривой
            let (response, painter) = ui.allocate_painter(
                egui::vec2(600.0, 400.0),
                Sense::click_and_drag(),
            );
            
            // Обработка взаимодействия с кривой
            self.handle_curve_interaction(&response);
            
            // Отрисовка кривой
            self.draw_curve(&painter, response.rect);
            
            // Информация о выбранной точке
            if let Some(index) = self.selected_point {
                let curve = self.curve.lock().unwrap();
                if let Some(point) = curve.control_points.get(index) {
                    ui.label(format!("Точка {}: ({:.1}, {:.1})", index, point.position.0, point.position.1));
                }
            }
            
            // Кнопки управления точками
            ui.horizontal(|ui| {
                if ui.button("➕ Добавить точку").clicked() {
                    if let Some(hover_pos) = response.hover_pos() {
                        self.add_control_point(hover_pos, response.rect);
                    }
                }
                
                if ui.button("❌ Удалить точку").clicked() {
                    self.remove_selected_point();
                }
                
                if ui.button("🔄 Сброс").clicked() {
                    self.reset_curve();
                }
            });
        });
    }
    
    fn draw_test_panel(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label("🧪 Тестирование кривой");
            
            // Слайдер для входного velocity
            ui.horizontal(|ui| {
                ui.label("Input Velocity:");
                ui.add(
                    egui::Slider::new(&mut self.midi_state.input_velocity, 0..=127)
                        .show_value(false)
                );
                ui.label(format!("{}", self.midi_state.input_velocity));
            });
            
            // Автоматическое обновление выходного значения
            self.test_curve();
            
            // Отображение результата
            ui.horizontal(|ui| {
                ui.label("Output Velocity:");
                ui.label(format!("{}", self.midi_state.output_velocity));
            });
            
            // Визуальная индикация
            ui.vertical(|ui| {
                ui.label("Визуальная индикация:");
                
                let bar_width = 200.0;
                
                // Полоса входного значения
                ui.horizontal(|ui| {
                    ui.label("In:");
                    let input_ratio = self.midi_state.input_velocity as f32 / 127.0;
                    ui.add_sized(
                        [bar_width, 20.0],
                        egui::widgets::ProgressBar::new(input_ratio)
                            .fill(egui::Color32::from_rgb(100, 100, 200))
                    );
                });
                
                // Полоса выходного значения
                ui.horizontal(|ui| {
                    ui.label("Out:");
                    let output_ratio = self.midi_state.output_velocity as f32 / 127.0;
                    ui.add_sized(
                        [bar_width, 20.0],
                        egui::widgets::ProgressBar::new(output_ratio)
                            .fill(egui::Color32::from_rgb(100, 200, 100))
                    );
                });
            });
        });
    }
    
    fn draw_presets_panel(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label("🎚️ Пресеты");
            
            // Получаем список пресетов
            let preset_names = self.preset_manager.lock().unwrap().get_preset_names();
            
            // Показываем текущий выбранный пресет
            if let Some(selected) = &self.selected_preset {
                ui.label(format!("Текущий: {}", selected));
            } else {
                ui.label("Текущий: (пользовательская кривая)");
            }
            
            ui.separator();
            
            // Кнопки управления пресетами
            ui.horizontal(|ui| {
                if ui.button("Загрузить").clicked() {
                    if !preset_names.is_empty() {
                        self.load_preset(&preset_names[0]);
                    }
                }
                
                if ui.button("Сохранить").clicked() {
                    // TODO: Показать диалог сохранения
                }
                
                if ui.button("Удалить").clicked() {
                    if let Some(selected_preset) = &self.selected_preset {
                        if let Err(e) = self.preset_manager.lock().unwrap().remove_preset(selected_preset) {
                            eprintln!("Ошибка удаления пресета: {}", e);
                        }
                    }
                }
            });
            
            ui.separator();
            
            // Список доступных пресетов
            ui.label("Доступные пресеты:");
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for preset_name in preset_names {
                        let is_selected = self.selected_preset.as_ref() == Some(&preset_name);
                        if ui.selectable_label(is_selected, &preset_name).clicked() {
                            self.selected_preset = Some(preset_name);
                        }
                    }
                });
        });
    }
    
    fn draw_midi_panel(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label("🎵 MIDI настройки");
            
            ui.label("⚠️ Полная MIDI поддержка будет добавлена в следующей версии");
            ui.label("Пока доступно только тестирование кривой");
            
            // Информация о MIDI портах (заглушка)
            ui.horizontal(|ui| {
                ui.label("Входные порты:");
                ui.label("Не найдены");
            });
            
            ui.horizontal(|ui| {
                ui.label("Выходные порты:");
                ui.label("Не найдены");
            });
        });
    }
    
    fn handle_curve_interaction(&mut self, response: &Response) {
        // Клик для выбора точки
        if response.clicked() {
            if let Some(hover_pos) = response.hover_pos() {
                self.selected_point = self.find_point_at(hover_pos, response.rect);
            }
        }
        
        // Перетаскивание точки
        if response.dragged() {
            if self.selected_point.is_some() {
                if let Some(hover_pos) = response.hover_pos() {
                    self.update_selected_point(hover_pos, response.rect);
                }
            }
        }
        
        // Двойной клик для добавления точки
        if response.double_clicked() {
            if let Some(hover_pos) = response.hover_pos() {
                self.add_control_point(hover_pos, response.rect);
            }
        }
        
        // Правый клик для удаления точки
        if response.secondary_clicked() {
            if let Some(hover_pos) = response.hover_pos() {
                if let Some(point_idx) = self.find_point_at(hover_pos, response.rect) {
                    self.selected_point = Some(point_idx);
                    self.remove_selected_point();
                }
            }
        }
    }
    
    fn draw_curve(&self, painter: &Painter, rect: Rect) {
        // Отрисовка сетки
        self.draw_grid(painter, rect);
        
        // Отрисовка кривой
        self.draw_bezier_curve(painter, rect);
        
        // Отрисовка контрольных точек
        self.draw_control_points(painter, rect);
    }
    
    fn draw_grid(&self, painter: &Painter, rect: Rect) {
        let grid_color = Color32::from_gray(40);
        
        // Вертикальные линии
        for i in 0..=10 {
            let x = rect.left() + rect.width() * i as f32 / 10.0;
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0, grid_color),
            );
        }
        
        // Горизонтальные линии
        for i in 0..=10 {
            let y = rect.top() + rect.height() * i as f32 / 10.0;
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(1.0, grid_color),
            );
        }
        
        // Оси
        let axis_color = Color32::from_gray(100);
        painter.line_segment(
            [Pos2::new(rect.left(), rect.bottom()), Pos2::new(rect.right(), rect.bottom())],
            Stroke::new(2.0, axis_color),
        );
        painter.line_segment(
            [Pos2::new(rect.left(), rect.top()), Pos2::new(rect.left(), rect.bottom())],
            Stroke::new(2.0, axis_color),
        );
    }
    
    fn draw_bezier_curve(&self, painter: &Painter, rect: Rect) {
        let curve = self.curve.lock().unwrap();
        
        if curve.control_points.len() < 2 {
            return;
        }
        
        let mut points = Vec::new();
        
        // Используем кэшированные значения кривой для отрисовки
        let cached_values = curve.get_cached_values();
        
        for (x, y) in cached_values {
            let x_norm = *x / 127.0;
            let screen_x = rect.left() + x_norm * rect.width();
            let screen_y = rect.bottom() - (y / 127.0) * rect.height();
            
            points.push(Pos2::new(screen_x, screen_y));
        }
        
        // Отрисовка кривой
        painter.add(Shape::line(
            points,
            Stroke::new(3.0, Color32::from_rgb(100, 200, 255)),
        ));
    }
    
    fn draw_control_points(&self, painter: &Painter, rect: Rect) {
        let curve = self.curve.lock().unwrap();
        
        for (i, point) in curve.control_points.iter().enumerate() {
            let screen_pos = self.world_to_screen(Pos2::new(point.position.0, point.position.1), rect);
            
            // Цвет точки зависит от выбранности
            let color = if Some(i) == self.selected_point {
                Color32::from_rgb(255, 100, 100) // Красный для выбранной
            } else {
                Color32::from_rgb(255, 150, 150) // Розовый для обычной
            };
            
            // Отрисовка точки
            painter.circle_filled(screen_pos, 8.0, color);
            painter.circle_stroke(screen_pos, 8.0, Stroke::new(2.0, Color32::BLACK));
            
            // Номер точки
            painter.text(
                screen_pos + egui::vec2(12.0, -12.0),
                egui::Align2::LEFT_TOP,
                i.to_string(),
                egui::FontId::default(),
                Color32::WHITE,
            );
        }
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    
    // Инициализация приложения с обработкой ошибок
    let app_result = MidiCurvesApp::new();
    if let Err(e) = app_result {
        eprintln!("Ошибка инициализации: {}", e);
        return Ok(());
    }
    let app = app_result.unwrap();
    
    eframe::run_native(
        "MIDI Curves - Standalone",
        options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
}