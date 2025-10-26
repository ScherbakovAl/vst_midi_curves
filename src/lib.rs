//! MIDI Curves VST3 Plugin
//!
//! VST3 плагин для обработки MIDI velocity с настраиваемыми кривыми Безье
//! Включает систему сохранения и загрузки настроек

use std::sync::{Arc, Mutex};

use nih_plug::prelude::*;
use nih_plug_egui::{EguiState, egui};

use crate::curve::DualCurve;
use crate::midi_simple::SimpleMidiManager;
use crate::presets::PresetManager;
use crate::settings::SettingsManager;

// Подключаем все необходимые модули
mod curve;
mod presets;
mod midi;
mod midi_simple;
mod settings;

// Параметры плагина
#[derive(Params)]
struct MidiCurvesParams {
    /// Количество контрольных точек в кривой
    #[id = "control_points_count"]
    control_points_count: IntParam,
}

// Основная структура плагина
struct MidiCurvesPlugin {
    /// Процессор кривой Безье (две кривые: NoteOn и NoteOff)
    dual_curve_processor: Arc<Mutex<DualCurve>>,
    
    /// Простой MIDI менеджер
    midi_manager: Arc<Mutex<SimpleMidiManager>>,
    
    /// Система пресетов
    preset_manager: Arc<Mutex<PresetManager>>,
    
    /// Менеджер настроек приложения
    settings_manager: SettingsManager,
    
    /// Состояние GUI
    gui_state: Arc<Mutex<GuiState>>,
}

// Структура для GUI состояния
struct GuiController {
    dual_curve_processor: Arc<Mutex<DualCurve>>,
    midi_manager: Arc<Mutex<SimpleMidiManager>>,
    preset_manager: Arc<Mutex<PresetManager>>,
    gui_state: Arc<Mutex<GuiState>>,
    settings_manager: SettingsManager,
}

// Состояние GUI для VST3 редактора
#[derive(Default)]
struct GuiState {
    selected_point: Option<usize>,
    is_dragging: bool,
    last_mouse_pos: Option<egui::Pos2>,
}

impl Default for MidiCurvesPlugin {
    fn default() -> Self {
        // Создаем менеджер настроек (загружает сохраненные настройки если есть)
        let settings_manager = SettingsManager::new().unwrap_or_default();
        
        // Восстанавливаем DualCurve из настроек или создаем новый
        let dual_curve_processor = Arc::new(Mutex::new(settings_manager.restore_to_dual_curve()));
        
        // Создаем MIDI менеджер
        let midi_manager = Arc::new(Mutex::new(SimpleMidiManager::new(dual_curve_processor.clone())));
        
        // Создаем систему пресетов
        let preset_manager = Arc::new(Mutex::new(PresetManager::new().unwrap()));
        
        // Добавляем встроенные пресеты
        {
            let mut preset_manager = preset_manager.lock().unwrap();
            preset_manager.create_builtin_presets().unwrap();
        }
        
        Self {
            dual_curve_processor,
            midi_manager,
            preset_manager,
            settings_manager: settings_manager.clone(),
            gui_state: Arc::new(Mutex::new(GuiState::default())),
        }
    }
}

impl Plugin for MidiCurvesPlugin {
    const NAME: &'static str = "MIDI Curves";
    const VENDOR: &'static str = "Your Company";
    const URL: &'static str = "https://yourcompany.com";
    const EMAIL: &'static str = "your@email.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // Это MIDI плагин, используем пустой audio layout
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[];

    // Настраиваем MIDI конфигурацию
    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::Basic;
    
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type BackgroundTask = ();
    type SysExMessage = ();

    fn params(&self) -> Arc<dyn Params> {
        // Создаем простую структуру параметров
        Arc::new(MidiCurvesParams {
            control_points_count: IntParam::new(
                "Control Points",
                2,
                IntRange::Linear { min: 2, max: 16 },
            ),
        }) as Arc<dyn Params>
    }

fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
    let egui_state = EguiState::from_size(1200, 800);
    let controller = GuiController {
        dual_curve_processor: self.dual_curve_processor.clone(),
        midi_manager: self.midi_manager.clone(),
        preset_manager: self.preset_manager.clone(),
        gui_state: self.gui_state.clone(),
        settings_manager: self.settings_manager.clone(),
    };
    
    nih_plug_egui::create_egui_editor(
        egui_state,
        controller,
        |egui_ctx, controller| {
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                controller.draw_plugin_ui(ui);
            });
        },
        |_ctx, _setter, _controller| {
            // Empty update function
        },
    )
}

fn process(
        &mut self,
        _buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Обрабатываем MIDI события
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn {
                    timing,
                    voice_id,
                    channel,
                    note,
                    velocity,
                    ..
                } => {
                    // Применяем кривую к velocity
                    let processed_velocity = {
                        let mut curve = self.dual_curve_processor.lock().unwrap();
                        curve.process_note_on_velocity((velocity * 127.0) as u8) as f32 / 127.0
                    };

                    context.send_event(NoteEvent::NoteOn {
                        timing,
                        voice_id,
                        channel,
                        note,
                        velocity: processed_velocity,
                    });
                }
                
                NoteEvent::NoteOff {
                    timing,
                    voice_id,
                    channel,
                    note,
                    velocity,
                    ..
                } => {
                    // Применяем кривую для NoteOff
                    let processed_velocity = {
                        let mut curve = self.dual_curve_processor.lock().unwrap();
                        curve.process_note_off_velocity((velocity * 127.0) as u8) as f32 / 127.0
                    };

                    context.send_event(NoteEvent::NoteOff {
                        timing,
                        voice_id,
                        channel,
                        note,
                        velocity: processed_velocity,
                    });
                }
                
                _ => {
                    // Все остальные события пропускаем без изменений
                    context.send_event(event);
                }
            }
        }

        ProcessStatus::Normal
    }
}

// Implement required traits for VST3 plugin
impl Vst3Plugin for MidiCurvesPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"MidiCurvesVST3!!";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[];
}

impl ClapPlugin for MidiCurvesPlugin {
    const CLAP_ID: &'static str = "com.yourcompany.midicurves";
    const CLAP_MANUAL_URL: Option<&'static str> = Some("https://yourcompany.com");
    const CLAP_DESCRIPTION: Option<&'static str> = Some("MIDI velocity processing plugin");
    const CLAP_SUPPORT_URL: Option<&'static str> = Some("https://yourcompany.com");
    
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Utility
    ];
}

impl GuiController {
    /// Отрисовка GUI плагина
    fn draw_plugin_ui(&self, ui: &mut egui::Ui) {
        ui.set_min_size(egui::vec2(1200.0, 800.0));
        
        // Заголовок
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("🎵 MIDI Curves VST3 Plugin").size(20.0));
        });
        
        ui.add_space(10.0);
        
        // Основной layout
        egui::SidePanel::left("left_panel")
            .default_width(600.0)
            .show_inside(ui, |ui| {
                self.draw_curve_editor(ui);
            });
        
        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.draw_control_panel(ui);
        });
    }
    
    /// Редактор кривой
    fn draw_curve_editor(&self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("🎯 Редактор кривой Безье").size(16.0));
        ui.add_space(5.0);
        
        // Canvas для графика
        let (response, painter) = ui.allocate_painter(
            egui::vec2(580.0, 500.0),
            egui::Sense::click_and_drag(),
        );
        
        // Обработка взаимодействий с кривой
        self.handle_curve_interaction(&response);
        
        // Отрисовка кривой
        self.draw_bezier_curve(&painter, response.rect);
        
        ui.add_space(10.0);
        
        // Информация о выбранной точке
        if let Some(index) = self.gui_state.lock().unwrap().selected_point {
            let curve = self.dual_curve_processor.lock().unwrap();
            if let Some(point) = curve.note_on_curve.control_points.get(index) {
                ui.label(format!(
                    "🎯 Выбрана точка {}: ({:.1}, {:.1})",
                    index,
                    point.position.0,
                    point.position.1
                ));
            }
        } else {
            ui.label("🎯 Точка не выбрана - кликните по кривой для выбора");
        }
        
        ui.add_space(5.0);
        
        // Кнопки управления точками
        ui.horizontal(|ui| {
            if ui.button("➕ Добавить точку").clicked() {
                if let Some(hover_pos) = response.hover_pos() {
                    let world_pos = self.screen_to_world(hover_pos, response.rect);
                    let mut curve = self.dual_curve_processor.lock().unwrap();
                    curve.add_note_on_point((world_pos.x, world_pos.y));
                }
            }
            
            if ui.button("❌ Удалить точку").clicked() {
                if let Some(index) = self.gui_state.lock().unwrap().selected_point {
                    let mut curve = self.dual_curve_processor.lock().unwrap();
                    curve.remove_note_on_point(index);
                    self.gui_state.lock().unwrap().selected_point = None;
                }
            }
            
            if ui.button("🔄 Сброс к линейной").clicked() {
                let mut curve = self.dual_curve_processor.lock().unwrap();
                curve.reset_to_linear();
                self.gui_state.lock().unwrap().selected_point = None;
            }
        });
    }
    
    /// Панель управления
    fn draw_control_panel(&self, ui: &mut egui::Ui) {
        // Тест кривой
        ui.group(|ui| {
            ui.label(egui::RichText::new("🎯 Тест кривой").size(14.0));
            
            let mut test_velocity = 64.0;
            ui.horizontal(|ui| {
                ui.label("Velocity:");
                ui.add(egui::Slider::new(&mut test_velocity, 0.0..=127.0).show_value(false));
                ui.label(format!("{}", test_velocity as i32));
            });
            
            let output_velocity = {
                let mut curve = self.dual_curve_processor.lock().unwrap();
                curve.note_on_curve.evaluate(test_velocity) as i32
            };
            
            ui.add_space(5.0);
            
            // Визуальная индикация
            ui.vertical(|ui| {
                let bar_width = 200.0;
                
                ui.horizontal(|ui| {
                    ui.label("In:");
                    let input_ratio = test_velocity / 127.0;
                    ui.add_sized(
                        [bar_width, 12.0],
                        egui::widgets::ProgressBar::new(input_ratio)
                            .fill(egui::Color32::from_rgb(100, 100, 200))
                    );
                });
                
                ui.horizontal(|ui| {
                    ui.label("Out:");
                    let output_ratio = output_velocity as f32 / 127.0;
                    ui.add_sized(
                        [bar_width, 12.0],
                        egui::widgets::ProgressBar::new(output_ratio)
                            .fill(egui::Color32::from_rgb(100, 200, 100))
                    );
                });
                
                ui.label(format!("Output: {}", output_velocity));
            });
        });
        
        ui.add_space(10.0);
        
        // Панель пресетов
        ui.group(|ui| {
            ui.label(egui::RichText::new("📁 Пресеты").size(14.0));
            
            let preset_names = self.preset_manager.lock().unwrap().get_preset_names();
            
            ui.horizontal(|ui| {
                ui.label("Выбранный пресет:");
                
                egui::ComboBox::from_id_source("preset_selector")
                    .selected_text("Linear")
                    .show_ui(ui, |ui| {
                        for preset_name in preset_names {
                            if ui.selectable_label(false, &preset_name).clicked() {
                                // Загружаем пресет
                                let mut curve = self.dual_curve_processor.lock().unwrap();
                                if let Some(preset) = self.preset_manager.lock().unwrap().get_preset(&preset_name) {
                                    curve.load_from_preset(&preset);
                                }
                            }
                        }
                    });
            });
        });
        
        ui.add_space(10.0);
        
        // Информация о плагине
        ui.group(|ui| {
            ui.label(egui::RichText::new("ℹ️ Информация").size(14.0));
            ui.label(format!("Версия: {}", env!("CARGO_PKG_VERSION")));
            ui.label("Платформа: VST3 Standalone");
            
            let curve = self.dual_curve_processor.lock().unwrap();
            ui.label(format!("Контрольных точек: {}", curve.note_on_curve.control_points.len()));
        });
    }
    
    /// Обработка взаимодействий с кривой
    fn handle_curve_interaction(&self, response: &egui::Response) {
        let mut gui_state = self.gui_state.lock().unwrap();
        
        // Клик для выбора точки
        if response.clicked() {
            if let Some(hover_pos) = response.hover_pos() {
                gui_state.selected_point = self.find_point_at(hover_pos, response.rect);
            }
        }
        
        // Перетаскивание точки
        if response.dragged() {
            if let Some(selected_index) = gui_state.selected_point {
                if let Some(hover_pos) = response.hover_pos() {
                    let world_pos = self.screen_to_world(hover_pos, response.rect);
                    let mut curve = self.dual_curve_processor.lock().unwrap();
                    curve.update_note_on_point(selected_index, (world_pos.x, world_pos.y));
                }
            }
        }
        
        // Отслеживание начала перетаскивания
        if !gui_state.is_dragging && response.dragged() && gui_state.selected_point.is_some() {
            gui_state.is_dragging = true;
        }
        
        // Сохранение настроек при отпускании кнопки мыши (drag release)
        if response.drag_released() && gui_state.is_dragging {
            // Мышь отпущена - сохраняем настройки
            gui_state.is_dragging = false;
            if let Err(e) = self.auto_save_settings() {
                eprintln!("Ошибка автосохранения настроек VST3: {}", e);
            }
        }
        
        // Двойной клик для добавления точки
        if response.double_clicked() {
            if let Some(hover_pos) = response.hover_pos() {
                let world_pos = self.screen_to_world(hover_pos, response.rect);
                let mut curve = self.dual_curve_processor.lock().unwrap();
                curve.add_note_on_point((world_pos.x, world_pos.y));
            }
        }
    }
    
    /// Отрисовка кривой Безье
    fn draw_bezier_curve(&self, painter: &egui::Painter, rect: egui::Rect) {
        // Отрисовка сетки
        self.draw_grid(painter, rect);
        
        let mut curve = self.dual_curve_processor.lock().unwrap();
        if curve.note_on_curve.control_points.len() < 2 {
            return;
        }
        
        // Генерируем точки кривой
        let mut curve_points = Vec::new();
        for i in 0..=128 {
            let x_input = i as f32;
            let y_output = curve.note_on_curve.evaluate(x_input);
            
            let screen_x = rect.left() + (x_input / 127.0) * rect.width();
            let screen_y = rect.bottom() - (y_output / 127.0) * rect.height();
            
            curve_points.push(egui::pos2(screen_x, screen_y));
        }
        
        // Рисуем кривую
        if curve_points.len() >= 2 {
            painter.add(egui::Shape::line(
                curve_points,
                egui::Stroke::new(3.0, egui::Color32::from_rgb(100, 200, 255))
            ));
        }
        
        // Рисуем контрольные точки
        let gui_state = self.gui_state.lock().unwrap();
        for (i, point) in curve.note_on_curve.control_points.iter().enumerate() {
            let screen_pos = self.world_to_screen(
                egui::pos2(point.position.0, point.position.1),
                rect
            );
            
            let color = if Some(i) == gui_state.selected_point {
                egui::Color32::from_rgb(255, 100, 100)
            } else {
                egui::Color32::from_rgb(255, 150, 150)
            };
            
            painter.circle_filled(screen_pos, 6.0, color);
            painter.circle_stroke(screen_pos, 6.0, egui::Stroke::new(1.0, egui::Color32::BLACK));
            
            // Номер точки
            painter.text(
                screen_pos + egui::vec2(8.0, -8.0),
                egui::Align2::LEFT_TOP,
                i.to_string(),
                egui::FontId::default(),
                egui::Color32::WHITE,
            );
        }
    }
    
    /// Отрисовка сетки
    fn draw_grid(&self, painter: &egui::Painter, rect: egui::Rect) {
        let grid_color = egui::Color32::from_gray(40);
        
        // Вертикальные линии
        for i in 0..=10 {
            let x = rect.left() + rect.width() * i as f32 / 10.0;
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1.0, grid_color),
            );
        }
        
        // Горизонтальные линии
        for i in 0..=10 {
            let y = rect.top() + rect.height() * i as f32 / 10.0;
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(1.0, grid_color),
            );
        }
        
        // Оси
        let axis_color = egui::Color32::from_gray(100);
        painter.line_segment(
            [egui::pos2(rect.left(), rect.bottom()), egui::pos2(rect.right(), rect.bottom())],
            egui::Stroke::new(2.0, axis_color),
        );
        painter.line_segment(
            [egui::pos2(rect.left(), rect.top()), egui::pos2(rect.left(), rect.bottom())],
            egui::Stroke::new(2.0, axis_color),
        );
    }
    
    /// Конвертация экранных координат в мировые
    fn screen_to_world(&self, screen_pos: egui::Pos2, rect: egui::Rect) -> egui::Pos2 {
        egui::pos2(
            (screen_pos.x - rect.left()) / rect.width() * 127.0,
            (rect.bottom() - screen_pos.y) / rect.height() * 127.0,
        )
    }
    
    /// Конвертация мировых координат в экранные
    fn world_to_screen(&self, world_pos: egui::Pos2, rect: egui::Rect) -> egui::Pos2 {
        egui::pos2(
            rect.left() + world_pos.x / 127.0 * rect.width(),
            rect.bottom() - world_pos.y / 127.0 * rect.height(),
        )
    }
    
    /// Поиск точки под курсором
    fn find_point_at(&self, screen_pos: egui::Pos2, rect: egui::Rect) -> Option<usize> {
        const CLICK_RADIUS: f32 = 12.0;
        let curve = self.dual_curve_processor.lock().unwrap();
        for (i, point) in curve.note_on_curve.control_points.iter().enumerate() {
            let point_screen = self.world_to_screen(
                egui::pos2(point.position.0, point.position.1),
                rect
            );
            if screen_pos.distance(point_screen) < CLICK_RADIUS {
                return Some(i);
            }
        }
        None
    }
}

// Реализация плагина
impl MidiCurvesPlugin {
    /// Сохраняет текущие настройки плагина
    pub fn save_settings(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Обновляем состояние кривых в настройках
        self.settings_manager.update_from_dual_curve(&self.dual_curve_processor.lock().unwrap());
        
        // Сохраняем настройки в файл
        self.settings_manager.save()
    }
    
    /// Восстанавливает настройки плагина
    pub fn load_settings(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Восстанавливаем DualCurve из настроек
        let restored_curve = self.settings_manager.restore_to_dual_curve();
        *self.dual_curve_processor.lock().unwrap() = restored_curve;
        
        Ok(())
    }
    
    /// Сбрасывает плагин к настройкам по умолчанию
    pub fn reset_to_defaults(&mut self) {
        self.settings_manager.reset_to_default();
        
        // Применяем сброшенные настройки
        let _ = self.load_settings();
    }
}

// Реализация GuiController
impl GuiController {
    /// Автоматически сохраняет настройки если включено автосохранение
    fn auto_save_settings(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.settings_manager.is_auto_save_enabled() {
            // Обновляем состояние кривых в настройках
            let dual_curve = self.dual_curve_processor.lock().unwrap();
            let mut settings_manager = self.settings_manager.clone();
            settings_manager.update_from_dual_curve(&dual_curve);
            
            // Сохраняем настройки в файл
            settings_manager.save()
        } else {
            Ok(())
        }
    }
}

// Экспорт плагина
nih_plug::nih_export_vst3!(MidiCurvesPlugin);
