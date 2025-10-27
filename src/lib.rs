//! MIDI Curves VST3 Plugin
//!
//! VST3 плагин для обработки MIDI velocity с настраиваемыми кривыми Безье
//! Включает систему сохранения и загрузки настроек

use std::num::NonZeroU32;
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

/// Буфер для hi-res MIDI сообщений в VST (CC#88 + NoteOn/Off)
#[derive(Debug, Clone)]
struct VstHiResBuffer {
    channel: Option<u8>,
    lower_bits: Option<u8>,
}

impl VstHiResBuffer {
    fn new() -> Self {
        Self {
            channel: None,
            lower_bits: None,
        }
    }
    
    fn store_cc88(&mut self, channel: u8, lower_bits: u8) {
        self.channel = Some(channel);
        self.lower_bits = Some(lower_bits);
    }
    
    fn extract(&mut self, channel: u8) -> Option<u8> {
        if let (Some(buffered_channel), Some(lower)) = (self.channel, self.lower_bits) {
            if buffered_channel == channel {
                self.clear();
                return Some(lower);
            }
        }
        self.clear();
        None
    }
    
    fn clear(&mut self) {
        self.channel = None;
        self.lower_bits = None;
    }
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
    
    /// Буфер для hi-res MIDI сообщений
    hi_res_buffer: Arc<Mutex<VstHiResBuffer>>,
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
        // VST3: БЕЗОПАСНАЯ инициализация - используем только память, БЕЗ файловых операций
        // Это предотвращает падение DAW из-за ограничений песочницы
        let settings_manager = SettingsManager::new_vst3_safe();
        
        // Восстанавливаем DualCurve из настроек или создаем новый
        let dual_curve_processor = Arc::new(Mutex::new(settings_manager.restore_to_dual_curve()));
        
        // Создаем MIDI менеджер
        let midi_manager = Arc::new(Mutex::new(SimpleMidiManager::new(dual_curve_processor.clone())));
        
        // VST3: Создаем систему пресетов БЕЗ операций с файловой системой
        // Используем пустой менеджер и добавляем встроенные пресеты только в памяти
        let preset_manager = Arc::new(Mutex::new(PresetManager::new_empty()));
        
        // Добавляем встроенные пресеты в память (без сохранения на диск)
        {
            let mut preset_mgr = preset_manager.lock().unwrap();
            // Создаем встроенные пресеты без сохранения на диск
            if let Err(e) = preset_mgr.create_builtin_presets_in_memory() {
                eprintln!("VST3: Ошибка создания встроенных пресетов: {}. Продолжаем без них.", e);
            }
        }
        
        // Восстанавливаем настройку hi_res из сохраненных настроек
        {
            let mut dual_curve = dual_curve_processor.lock().unwrap();
            dual_curve.set_hi_res_enabled(settings_manager.is_hi_res_enabled());
        }
        
        Self {
            dual_curve_processor,
            midi_manager,
            preset_manager,
            settings_manager: settings_manager.clone(),
            gui_state: Arc::new(Mutex::new(GuiState::default())),
            hi_res_buffer: Arc::new(Mutex::new(VstHiResBuffer::new())),
        }
    }
}

impl Plugin for MidiCurvesPlugin {
    const NAME: &'static str = "MIDI Curves";
    const VENDOR: &'static str = "Rust VST Developer";
    const URL: &'static str = "https://github.com/vst-midi-curves";
    const EMAIL: &'static str = "developer@vst-plugins.org";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // MIDI-only плагин с минимальным стерео layout для совместимости с Reaper
    // Reaper требует хотя бы стерео вход/выход, даже для MIDI-only плагинов
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];

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
    // ВРЕМЕННО ОТКЛЮЧЕНО: GUI вызывает панику при загрузке в DAW
    // Проблема: egui пытается обратиться к шрифтам до Context::run()
    // TODO: Найти способ безопасной инициализации egui в VST3
    None
    
    /*
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
    */
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
                NoteEvent::MidiCC {
                    timing,
                    channel,
                    cc,
                    value,
                    ..
                } => {
                    // Проверяем CC#88 для hi-res режима
                    if cc == 88 {
                        let hi_res_enabled = self.dual_curve_processor.lock().unwrap().is_hi_res_enabled();
                        if hi_res_enabled {
                            // Hi-res включен - сохраняем CC#88 в буфер
                            let ll = (value * 127.0) as u8;
                            self.hi_res_buffer.lock().unwrap().store_cc88(channel, ll);
                        }
                        // В обоих случаях не отправляем CC#88 дальше
                        // (в стандартном режиме просто игнорируем, в hi-res буферизуем)
                        continue;
                    }
                    // Остальные CC пропускаем без изменений
                    context.send_event(event);
                }
                
                NoteEvent::NoteOn {
                    timing,
                    voice_id,
                    channel,
                    note,
                    velocity,
                    ..
                } => {
                    let mut curve = self.dual_curve_processor.lock().unwrap();
                    let hi_res_enabled = curve.is_hi_res_enabled();
                    
                    if hi_res_enabled {
                        // Проверяем буфер на наличие CC#88
                        let lower_bits = self.hi_res_buffer.lock().unwrap().extract(channel);
                        
                        // В hi-res режиме ВСЕГДА обрабатываем как 14-бит
                        // Если нет CC#88, используем LL=0
                        let ll = lower_bits.unwrap_or(0);
                        let hh = (velocity * 127.0) as u8;
                        let velocity_14bit = crate::curve::DualCurve::combine_14bit(ll, hh);
                        
                        // Обрабатываем через 14-битную кривую
                        let processed_14bit = curve.process_note_on_velocity_14bit(velocity_14bit);
                        
                        // Разделяем обратно на LL и HH
                        let (new_ll, new_hh) = crate::curve::DualCurve::split_14bit(processed_14bit);
                        
                        // Отправляем CC#88
                        context.send_event(NoteEvent::MidiCC {
                            timing,
                            channel,
                            cc: 88,
                            value: new_ll as f32 / 127.0,
                        });
                        
                        // Отправляем NoteOn
                        context.send_event(NoteEvent::NoteOn {
                            timing,
                            voice_id,
                            channel,
                            note,
                            velocity: new_hh as f32 / 127.0,
                        });
                    } else {
                        // Hi-res выключен - обычная обработка
                        let processed_velocity = curve.process_note_on_velocity((velocity * 127.0) as u8);
                        context.send_event(NoteEvent::NoteOn {
                            timing,
                            voice_id,
                            channel,
                            note,
                            velocity: processed_velocity as f32 / 127.0,
                        });
                    }
                }
                
                NoteEvent::NoteOff {
                    timing,
                    voice_id,
                    channel,
                    note,
                    velocity,
                    ..
                } => {
                    let mut curve = self.dual_curve_processor.lock().unwrap();
                    let hi_res_enabled = curve.is_hi_res_enabled();
                    
                    if hi_res_enabled {
                        // Проверяем буфер на наличие CC#88
                        let lower_bits = self.hi_res_buffer.lock().unwrap().extract(channel);
                        
                        // В hi-res режиме ВСЕГДА обрабатываем как 14-бит
                        // Если нет CC#88, используем LL=0
                        let ll = lower_bits.unwrap_or(0);
                        let hh = (velocity * 127.0) as u8;
                        let velocity_14bit = crate::curve::DualCurve::combine_14bit(ll, hh);
                        
                        // Обрабатываем через 14-битную кривую
                        let processed_14bit = curve.process_note_off_velocity_14bit(velocity_14bit);
                        
                        // Разделяем обратно на LL и HH
                        let (new_ll, new_hh) = crate::curve::DualCurve::split_14bit(processed_14bit);
                        
                        // Отправляем CC#88
                        context.send_event(NoteEvent::MidiCC {
                            timing,
                            channel,
                            cc: 88,
                            value: new_ll as f32 / 127.0,
                        });
                        
                        // Отправляем NoteOff
                        context.send_event(NoteEvent::NoteOff {
                            timing,
                            voice_id,
                            channel,
                            note,
                            velocity: new_hh as f32 / 127.0,
                        });
                    } else {
                        // Hi-res выключен - обычная обработка
                        let processed_velocity = curve.process_note_off_velocity((velocity * 127.0) as u8);
                        context.send_event(NoteEvent::NoteOff {
                            timing,
                            voice_id,
                            channel,
                            note,
                            velocity: processed_velocity as f32 / 127.0,
                        });
                    }
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
    // Категория для MIDI плагина - Fx|MIDI для MIDI эффектов
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Fx,
        Vst3SubCategory::Tools,
    ];
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
            ui.heading("🎵 MIDI Curves VST3 Plugin");
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
        ui.heading("🎯 Редактор кривой Безье");
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
            ui.strong("🎯 Тест кривой");
            
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
        
        // Панель настроек Hi-Res MIDI
        ui.group(|ui| {
            ui.strong("⚙️ MIDI Settings");
            
            let mut hi_res_enabled = self.dual_curve_processor.lock().unwrap().is_hi_res_enabled();
            
            if ui.checkbox(&mut hi_res_enabled, "Enable Hi-Res MIDI (14-bit velocity)").clicked() {
                // Обновляем состояние в dual_curve
                self.dual_curve_processor.lock().unwrap().set_hi_res_enabled(hi_res_enabled);
                
                // Обновляем в настройках (в памяти, без сохранения на диск в VST3)
                self.settings_manager.clone().set_hi_res_enabled(hi_res_enabled);
                // В VST3 автосохранение на диск отключено
                let _ = self.settings_manager.clone().save(); // No-op в VST3 режиме
            }
            
            ui.add_space(5.0);
            
            if hi_res_enabled {
                ui.colored_label(egui::Color32::from_rgb(100, 200, 100), "✓ Hi-Res mode: 14-bit velocity (0-16383)");
                ui.label("Format: CC#88 (LL) + NoteOn/Off (HH)");
            } else {
                ui.colored_label(egui::Color32::from_rgb(200, 200, 100), "Standard mode: 7-bit velocity (0-127)");
            }
        });
        
        ui.add_space(10.0);
        
        // Панель пресетов
        ui.group(|ui| {
            ui.strong("📁 Пресеты");
            
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
            ui.strong("ℹ️ Информация");
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
            // Мышь отпущена - обновляем настройки в памяти (без сохранения на диск в VST3)
            gui_state.is_dragging = false;
            // Автосохранение в VST3 отключено - настройки хранятся только в памяти
            let _ = self.auto_save_settings(); // Игнорируем результат, т.к. в VST3 это no-op
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
            
            // Номер точки убран для совместимости с VST3 (FontId вызывает панику до Context::run())
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
    /// В VST3 режиме только обновляет настройки в памяти
    pub fn save_settings(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Обновляем состояние кривых в настройках (в памяти)
        self.settings_manager.update_from_dual_curve(&self.dual_curve_processor.lock().unwrap());
        
        // В VST3 это no-op (не записывает на диск), в standalone сохраняет в файл
        self.settings_manager.save()
    }
    
    /// Восстанавливает настройки плагина из памяти
    pub fn load_settings(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Восстанавливаем DualCurve из настроек (из памяти в VST3)
        let restored_curve = self.settings_manager.restore_to_dual_curve();
        *self.dual_curve_processor.lock().unwrap() = restored_curve;
        
        Ok(())
    }
    
    /// Сбрасывает плагин к настройкам по умолчанию (только в памяти в VST3)
    pub fn reset_to_defaults(&mut self) {
        self.settings_manager.reset_to_default();
        
        // Применяем сброшенные настройки
        let _ = self.load_settings();
    }
}

// Реализация GuiController
impl GuiController {
    /// Автоматически сохраняет настройки если включено автосохранение
    /// В VST3 режиме только обновляет настройки в памяти без записи на диск
    fn auto_save_settings(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Обновляем состояние кривых в настройках (в памяти)
        let dual_curve = self.dual_curve_processor.lock().unwrap();
        let mut settings_manager = self.settings_manager.clone();
        settings_manager.update_from_dual_curve(&dual_curve);
        
        // В VST3 режиме save() является no-op (не записывает на диск)
        // В standalone режиме сохраняет в файл, если автосохранение включено
        if self.settings_manager.is_auto_save_enabled() {
            settings_manager.save()
        } else {
            Ok(())
        }
    }
}

// Экспорт плагина
nih_plug::nih_export_vst3!(MidiCurvesPlugin);
