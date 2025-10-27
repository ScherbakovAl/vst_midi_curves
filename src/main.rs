//! MIDI Curves - Standalone приложение для обработки MIDI velocity с настраиваемыми кривыми
//!
//! Теперь поддерживает отдельные кривые для NoteOn и NoteOff событий.
//! Использует DualCurve структуру для управления двумя кривыми одновременно.
//! Включает систему сохранения и загрузки настроек для каждой платформы.

use eframe::egui;
use std::sync::{Arc, Mutex};

// Подключаем модули проекта
mod curve;
mod presets;
mod midi;
mod midi_simple;
mod settings;

use curve::DualCurve;
use presets::{PresetManager, CurvePreset};
use midi::{MidiManager, MidiEvent, MidiStats};
use settings::SettingsManager;
use egui::{Pos2, Rect, Sense, Response, Painter, Color32, Stroke};

// MIDI события для отображения в GUI
#[derive(Debug, Clone)]
struct GuiMidiEvent {
    event: MidiEvent,
    timestamp: std::time::Instant,
    is_processed: bool,
}

// Главная структура приложения
struct MidiCurvesApp {
    // Ядро обработки кривых - теперь использует DualCurve для NoteOn и NoteOff
    dual_curve: Arc<Mutex<DualCurve>>,
    
    // MIDI менеджер для реальной обработки
    midi_manager: Arc<Mutex<MidiManager>>,
    
    midi_input_ports: Vec<String>,
    midi_output_ports: Vec<String>,
    selected_input_port: Option<String>,
    selected_output_port: Option<String>,
    midi_stats: MidiStats,
    
    // Состояние интерфейса
    selected_point: Option<usize>,
    is_dragging: bool,
    drag_start: Option<Pos2>,
    shift_pressed: bool,
    
    // Активная вкладка: true для NoteOn, false для NoteOff
    active_tab_note_on: bool,
    
    // Менеджер пресетов
    preset_manager: Arc<Mutex<PresetManager>>,
    selected_preset: Option<String>,
    
    // Менеджер настроек приложения
    settings_manager: SettingsManager,
}

impl MidiCurvesApp {
fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Создаем менеджер настроек (загружает сохраненные настройки если есть)
        let settings_manager = SettingsManager::new()?;
        
        // Создаем менеджер пресетов
        let preset_manager = Arc::new(Mutex::new(PresetManager::new()?));
        
        // Создаем встроенные пресеты если их нет
        {
            let mut manager = preset_manager.lock().unwrap();
            manager.create_builtin_presets()?;
        }
        
        // Восстанавливаем DualCurve из настроек или создаем новый
        let dual_curve_processor = Arc::new(Mutex::new(settings_manager.restore_to_dual_curve()));
        
        // Инициализируем приложение
        let mut app = Self {
            dual_curve: dual_curve_processor.clone(),
            midi_manager: Arc::new(Mutex::new(MidiManager::new(dual_curve_processor.clone()))),
            midi_input_ports: Vec::new(),
            midi_output_ports: Vec::new(),
            selected_input_port: None,
            selected_output_port: None,
            midi_stats: MidiStats::new(),
            
            selected_point: None,
            is_dragging: false,
            drag_start: None,
            shift_pressed: false,
            active_tab_note_on: settings_manager.get_active_curve_tab() == 0, // Восстанавливаем из настроек
            preset_manager,
            selected_preset: None,
            settings_manager,
        };
        
        // Запускаем полный MIDI менеджер
        {
            let mut midi_manager_mut = app.midi_manager.lock().unwrap();
            midi_manager_mut.start()?;
        }
        
        // Восстанавливаем последние выбранные MIDI порты
        app.restore_midi_ports_from_settings();
        
        // Восстанавливаем последний пресет если есть
        app.restore_last_preset_from_settings();
        
        // Обновляем список MIDI портов при запуске
        app.refresh_midi_ports();
        
        Ok(app)
    }
    
    /// Восстанавливает MIDI порты из сохраненных настроек с проверкой доступности
    fn restore_midi_ports_from_settings(&mut self) {
        // Сначала обновляем список доступных портов
        self.refresh_midi_ports();
        
        // Сохраняем значения из настроек в переменные, чтобы избежать заимствования
        let saved_input_port = self.settings_manager.get_last_input_port().map(|s| s.clone());
        let saved_output_port = self.settings_manager.get_last_output_port().map(|s| s.clone());
        
        // Восстанавливаем входной порт
        if let Some(input_port_name) = saved_input_port {
            // Проверяем, существует ли сохраненный порт в текущем списке доступных
            if self.midi_input_ports.iter().any(|port| port == &input_port_name) {
                self.selected_input_port = Some(input_port_name.clone());
                
                // Пытаемся подключиться к порту
                if let Err(e) = self.connect_input_port(&input_port_name) {
                    eprintln!("Ошибка восстановления входного порта '{}': {}", input_port_name, e);
                    self.selected_input_port = None; // Сбрасываем при ошибке
                }
            } else {
                // Сохраненный порт не найден - сбрасываем настройку
                eprintln!("Сохраненный входной порт '{}' недоступен", input_port_name);
                self.settings_manager.update_midi_ports(None, self.selected_output_port.clone());
                self.selected_input_port = None;
            }
        }
        
        // Восстанавливаем выходной порт
        if let Some(output_port_name) = saved_output_port {
            // Проверяем, существует ли сохраненный порт в текущем списке доступных
            if self.midi_output_ports.iter().any(|port| port == &output_port_name) {
                self.selected_output_port = Some(output_port_name.clone());
                
                // Пытаемся подключиться к порту
                if let Err(e) = self.connect_output_port(&output_port_name) {
                    eprintln!("Ошибка восстановления выходного порта '{}': {}", output_port_name, e);
                    self.selected_output_port = None; // Сбрасываем при ошибке
                }
            } else {
                // Сохраненный порт не найден - сбрасываем настройку
                eprintln!("Сохраненный выходной порт '{}' недоступен", output_port_name);
                self.settings_manager.update_midi_ports(self.selected_input_port.clone(), None);
                self.selected_output_port = None;
            }
        }
    }
    
    /// Восстанавливает последний пресет из настроек
    fn restore_last_preset_from_settings(&mut self) {
        if let Some(preset_name) = self.settings_manager.get_last_preset().cloned() {
            self.load_preset(&preset_name);
            self.selected_preset = Some(preset_name);
        }
    }
    
    /// Сохраняет текущие настройки приложения
    fn save_settings(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Обновляем состояние кривых в настройках
        self.settings_manager.update_from_dual_curve(&self.dual_curve.lock().unwrap());
        
        // Обновляем MIDI порты
        self.settings_manager.update_midi_ports(
            self.selected_input_port.clone(),
            self.selected_output_port.clone()
        );
        
        // Обновляем последний пресет
        self.settings_manager.update_last_preset(self.selected_preset.clone());
        
        // Сохраняем настройки в файл
        self.settings_manager.save()
    }
    
    /// Автоматически сохраняет настройки если включено автосохранение
    fn auto_save_settings(&mut self) {
        if self.settings_manager.is_auto_save_enabled() {
            if let Err(e) = self.save_settings() {
                eprintln!("Ошибка автосохранения настроек: {}", e);
            }
        }
    }
    
    /// Вызывается при закрытии приложения для сохранения настроек
    fn on_exit(&mut self) {
        if let Err(e) = self.save_settings() {
            eprintln!("Ошибка при сохранении настроек: {}", e);
        }
    }
    
fn process_input_velocity(&self, input_velocity: u8) -> u8 {
        let mut dual_curve = self.dual_curve.lock().unwrap();
        // По умолчанию используем NoteOn кривую для тестирования
        dual_curve.process_note_on_velocity(input_velocity)
    }
    
    // Тестирование кривой
    fn test_curve(&mut self) {
        let input_velocity = 64; // Тестовое значение
        let _output_velocity = self.process_input_velocity(input_velocity);
        // Velocity test completed silently
    }
    
    // Обновление списка MIDI портов
    fn refresh_midi_ports(&mut self) {
        // Используем полный MIDI менеджер
        let midi_manager = self.midi_manager.lock().unwrap();
        self.midi_input_ports = midi_manager.get_input_ports();
        self.midi_output_ports = midi_manager.get_output_ports();
        self.midi_stats = midi_manager.get_stats();
        
        // MIDI ports list updated silently
    }
    
    // Подключение к входному MIDI порту
    fn connect_input_port(&mut self, port_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut midi_manager = self.midi_manager.lock().unwrap();
        midi_manager.connect_input_port(port_name)?;
        self.selected_input_port = Some(port_name.to_string());
        
        // Автоматически сохраняем в настройки
        if self.settings_manager.is_auto_save_enabled() {
            self.settings_manager.update_midi_ports(
                Some(port_name.to_string()),
                self.selected_output_port.clone()
            );
            let _ = self.settings_manager.save();
        }
        
        Ok(())
    }
    
    // Подключение к выходному MIDI порту
    fn connect_output_port(&mut self, port_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut midi_manager = self.midi_manager.lock().unwrap();
        midi_manager.connect_output_port(port_name)?;
        self.selected_output_port = Some(port_name.to_string());
        
        // Автоматически сохраняем в настройки
        if self.settings_manager.is_auto_save_enabled() {
            self.settings_manager.update_midi_ports(
                self.selected_input_port.clone(),
                Some(port_name.to_string())
            );
            let _ = self.settings_manager.save();
        }
        
        Ok(())
    }
    
// Отключение всех MIDI портов
    fn disconnect_all_ports(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut midi_manager = self.midi_manager.lock().unwrap();
        midi_manager.disconnect_all_ports()?;
        self.selected_input_port = None;
        self.selected_output_port = None;
        
        // Сохраняем настройки
        if self.settings_manager.is_auto_save_enabled() {
            self.settings_manager.update_midi_ports(None, None);
            let _ = self.settings_manager.save();
        }
        
        Ok(())
    }
    
// Отключение входного MIDI порта
    fn disconnect_input_port(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut midi_manager = self.midi_manager.lock().unwrap();
        midi_manager.disconnect_input_port()?;
        self.selected_input_port = None;
        
        // Сохраняем настройки
        if self.settings_manager.is_auto_save_enabled() {
            self.settings_manager.update_midi_ports(None, self.selected_output_port.clone());
            let _ = self.settings_manager.save();
        }
        
        Ok(())
    }
    
    // Отключение выходного MIDI порта
    fn disconnect_output_port(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut midi_manager = self.midi_manager.lock().unwrap();
        midi_manager.disconnect_output_port()?;
        self.selected_output_port = None;
        
        // Сохраняем настройки
        if self.settings_manager.is_auto_save_enabled() {
            self.settings_manager.update_midi_ports(self.selected_input_port.clone(), None);
            let _ = self.settings_manager.save();
        }
        
        Ok(())
    }
    
    // Проверка активности MIDI
    fn is_midi_active(&self) -> bool {
        let midi_manager = self.midi_manager.lock().unwrap();
        midi_manager.is_active()
    }
    
    // Обновление списка MIDI событий для GUI
    fn update_midi_events(&mut self) {
        // В реальной реализации здесь события поступают через callbacks
        // Пока оставляем пустым для совместимости с простым менеджером
    }
    
    // Тестирование MIDI обработки
    fn test_midi_processing(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let _midi_manager = self.midi_manager.lock().unwrap();
        // Создаем тестовое MIDI событие
        let _test_event = MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 64,
            timestamp: std::time::Instant::now(),
        };
        
        // Generate test MIDI event silently
        
        Ok(())
    }
    
// Отправка тестового MIDI сообщения на подключенный выходной порт
    fn send_test_midi_message(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut dual_curve = self.dual_curve.lock().unwrap();
        
        // Create test MIDI NoteOn event
        let test_event = MidiEvent::NoteOn {
            channel: 0,
            note: 60, // Middle C
            velocity: 64, // Medium volume
            timestamp: std::time::Instant::now(),
        };
        
        // Apply NoteOn velocity curve to event
        let processed_event = {
            match test_event.clone() {
                MidiEvent::NoteOn { channel, note, velocity, timestamp } => {
                    let processed_velocity = dual_curve.process_note_on_velocity(velocity);
                    
                    MidiEvent::NoteOn {
                        channel,
                        note,
                        velocity: processed_velocity,
                        timestamp,
                    }
                }
                _ => test_event.clone(), // Other events not processed
            }
        };
        
        // Convert to MIDI data for sending
        let midi_data = processed_event.to_midi_data();
        
        // Send through full MIDI manager to output port
        let mut midi_manager = self.midi_manager.lock().unwrap();
        midi_manager.send_midi_data(&midi_data)?;
        
        // Also send corresponding NoteOff after some time
        std::thread::sleep(std::time::Duration::from_millis(200));
        
        let note_off_event = match test_event {
            MidiEvent::NoteOn { channel, note, velocity: _, timestamp: _ } => {
                // Apply NoteOff velocity curve to event
                let processed_note_off_velocity = dual_curve.process_note_off_velocity(64);
                
                MidiEvent::NoteOff {
                    channel,
                    note,
                    velocity: processed_note_off_velocity,
                    timestamp: std::time::Instant::now(),
                }
            }
            _ => unreachable!(), // Should be NoteOn
        };
        
        let note_off_data = note_off_event.to_midi_data();
        let _ = midi_manager.send_midi_data(&note_off_data);
        
        Ok(())
    }
    
// Загрузка пресета (применяется к активной вкладке)
    fn load_preset(&mut self, preset_name: &str) {
        // Сначала получаем пресет и клонируем его данные
        let preset_data = {
            let preset_manager = self.preset_manager.lock().unwrap();
            preset_manager.get_preset(preset_name).map(|preset| preset.clone())
        };
        
        if let Some(preset) = preset_data {
            {
                let mut dual_curve = self.dual_curve.lock().unwrap();
                let points = preset.to_control_points();
                
                if self.active_tab_note_on {
                    // Применяем к NoteOn кривой
                    dual_curve.note_on_curve.control_points = points.clone();
                    dual_curve.note_on_curve.dirty = true;
                } else {
                    // Применяем к NoteOff кривой
                    dual_curve.note_off_curve.control_points = points;
                    dual_curve.note_off_curve.dirty = true;
                }
            } // освобождаем замок здесь
            
            self.selected_preset = Some(preset_name.to_string());
            
            // Автоматически сохраняем изменения
            self.auto_save_settings();
        }
    }
    
    // Сохранение текущей кривой как пресета
    fn save_current_as_preset(&mut self, name: String, description: String) -> Result<(), Box<dyn std::error::Error>> {
        let dual_curve = self.dual_curve.lock().unwrap();
        
        let points = if self.active_tab_note_on {
            dual_curve.note_on_curve.control_points.clone()
        } else {
            dual_curve.note_off_curve.control_points.clone()
        };
        
        let preset = CurvePreset::new(name, description, points);
        self.preset_manager.lock().unwrap().add_preset(preset)?;
        
        Ok(())
    }
    
// Сброс к линейной кривой (обеих кривых)
fn reset_curve(&mut self) {
    {
        let mut dual_curve = self.dual_curve.lock().unwrap();
        dual_curve.reset_to_linear();
    } // освобождаем замок здесь
    
    self.selected_preset = None;
    self.selected_point = None;
    
    // Настройки сохранятся при закрытии приложения
}
    
// Добавление контрольной точки к активной кривой
fn add_control_point(&mut self, position: Pos2, rect: Rect) {
    let world_pos = self.screen_to_world(position, rect);
    {
        let mut dual_curve = self.dual_curve.lock().unwrap();
        
        if self.active_tab_note_on {
            dual_curve.add_note_on_point((world_pos.x, world_pos.y));
        } else {
            dual_curve.add_note_off_point((world_pos.x, world_pos.y));
        }
    } // освобождаем замок здесь
    
    // Настройки сохранятся при закрытии приложения
}
    
// Удаление выбранной точки из активной кривой
fn remove_selected_point(&mut self) -> bool {
    if let Some(index) = self.selected_point {
        let removed = {
            let mut dual_curve = self.dual_curve.lock().unwrap();
            if self.active_tab_note_on {
                dual_curve.remove_note_on_point(index)
            } else {
                dual_curve.remove_note_off_point(index)
            }
        }; // освобождаем замок здесь
        
        if removed {
            self.selected_point = None;
            // Настройки сохранятся при закрытии приложения
        }
        removed
    } else {
        false
    }
}
    
// Обновление позиции точки в активной кривой
fn update_selected_point(&mut self, position: Pos2, rect: Rect) {
    if let Some(index) = self.selected_point {
        let mut world_pos = self.screen_to_world(position, rect);
        
        {
            let dual_curve = self.dual_curve.lock().unwrap();
            let points = if self.active_tab_note_on {
                &dual_curve.note_on_curve.control_points
            } else {
                &dual_curve.note_off_curve.control_points
            };
            
            // При тонком перемещении (Shift) ограничиваем скорость перемещения
            if self.shift_pressed {
                if let Some(current_point) = points.get(index) {
                    let current_x = current_point.position.0;
                    let current_y = current_point.position.1;
                    
                    let dx = world_pos.x - current_x;
                    let dy = world_pos.y - current_y;
                    
                    // В тонком режиме двигаемся очень медленно
                    // Максимальный шаг 0.02 за одно обновление (сотые доли)
                    let max_fine_step = 0.02;
                    
                    world_pos = Pos2::new(
                        current_x + dx.signum() * dx.abs().min(max_fine_step),
                        current_y + dy.signum() * dy.abs().min(max_fine_step)
                    );
                }
            }
        }
        
        {
            let mut dual_curve = self.dual_curve.lock().unwrap();
            
            if self.active_tab_note_on {
                dual_curve.update_note_on_point(index, (world_pos.x, world_pos.y));
            } else {
                dual_curve.update_note_off_point(index, (world_pos.x, world_pos.y));
            }
        } // освобождаем замок здесь
        
        // Настройки будут сохранены при завершении перетаскивания (mouse release)
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
    
fn find_point_at(&self, screen_pos: Pos2, rect: Rect) -> Option<usize> {
        const CLICK_RADIUS: f32 = 12.0;
        let dual_curve = self.dual_curve.lock().unwrap();
        
        let points = if self.active_tab_note_on {
            &dual_curve.note_on_curve.control_points
        } else {
            &dual_curve.note_off_curve.control_points
        };
        
        for (i, point) in points.iter().enumerate() {
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
        // Обработка нажатий клавиш
        self.shift_pressed = ctx.input(|i| i.modifiers.shift);
        
        // Отрисовка главного окна
        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_main_ui(ui);
        });
        
        // Обычная перерисовка при необходимости (убираем принудительное)
        // ctx.request_repaint();
    }
}

impl MidiCurvesApp {
    fn draw_main_ui(&mut self, ui: &mut egui::Ui) {
        // Заголовок приложения
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("🎵 MIDI Curves Plugin - Dual Curves").size(18.0));
        });
        
        ui.add_space(10.0);
        
        // Вкладки для выбора между NoteOn и NoteOff кривыми
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("📊 Curve Type:").size(14.0));
            ui.add_space(5.0);
            
            // NoteOn вкладка
            let note_on_button = ui.button(if self.active_tab_note_on {
                "🎵 NoteOn (Active)"
            } else {
                "🎵 NoteOn"
            });
            if note_on_button.clicked() {
                self.active_tab_note_on = true;
                self.selected_point = None; // Сбрасываем выбранную точку при переключении
                
                // Сохраняем настройку активной вкладки
                self.settings_manager.update_active_curve_tab(0);
                if self.settings_manager.is_auto_save_enabled() {
                    let _ = self.settings_manager.save();
                }
            }
            
            ui.add_space(5.0);
            
            // NoteOff вкладка
            let note_off_button = ui.button(if !self.active_tab_note_on {
                "🔇 NoteOff (Active)"
            } else {
                "🔇 NoteOff"
            });
            if note_off_button.clicked() {
                self.active_tab_note_on = false;
                self.selected_point = None; // Сбрасываем выбранную точку при переключении
                
                // Сохраняем настройку активной вкладки
                self.settings_manager.update_active_curve_tab(1);
                if self.settings_manager.is_auto_save_enabled() {
                    let _ = self.settings_manager.save();
                }
            }
        });
        
        ui.add_space(10.0);
        
        // Основной layout с горизонтальным разделением
        ui.horizontal(|ui| {
            // Левая часть - график с подписью и управлением
            ui.vertical(|ui| {
                ui.set_min_width(600.0);
                
                // Заголовок графика
                let curve_type = if self.active_tab_note_on { "NoteOn" } else { "NoteOff" };
                ui.label(egui::RichText::new(format!("🎯 {} Curve Editor", curve_type)).size(16.0));
                
                // Область графика
                ui.add_space(5.0);
                let (response, painter) = ui.allocate_painter(
                    egui::vec2(600.0, 400.0),
                    Sense::click_and_drag(),
                );
                
                // Обработка взаимодействия с кривой
                self.handle_curve_interaction(&response);
                
                // Отрисовка кривой
                self.draw_curve(&painter, response.rect);
                
                ui.add_space(10.0);
                
                // Информация о выбранной точке
                if let Some(index) = self.selected_point {
                    let dual_curve = self.dual_curve.lock().unwrap();
                    let points = if self.active_tab_note_on {
                        &dual_curve.note_on_curve.control_points
                    } else {
                        &dual_curve.note_off_curve.control_points
                    };
                    
                    if let Some(point) = points.get(index) {
                        let format_str = if self.shift_pressed {
                            format!("🎯 Selected Point {}: ({:.2}, {:.2}) [FINE MODE]", index, point.position.0, point.position.1)
                        } else {
                            format!("🎯 Selected Point {}: ({:.1}, {:.1})", index, point.position.0, point.position.1)
                        };
                        ui.label(format_str);
                    }
                } else {
                    ui.label(format!("🎯 No point selected for {} curve - click on curve to select", curve_type));
                }
                
                ui.add_space(5.0);
                
                // Инструкции для работы с кривой
                ui.label(egui::RichText::new("Double click - add point. Right click - delete point").size(12.0));
                
                // Индикация режима тонкого перемещения
                if self.shift_pressed {
                    ui.colored_label(
                        egui::Color32::from_rgb(100, 200, 100),
                        "🔧 FINE MODE: Shift held - precise positioning (0.01 increments)"
                    );
                } else {
                    ui.colored_label(
                        egui::Color32::from_gray(120),
                        "💫 Hold Shift for fine control (0.01 increments)"
                    );
                }
                
                ui.add_space(5.0);
                
                // Кнопки управления
                ui.horizontal(|ui| {
                    if ui.button("🔄 Reset to Linear").clicked() {
                        self.reset_curve();
                    }
                });
                
                ui.add_space(5.0);
                
                // Информация о текущих точках
                let dual_curve = self.dual_curve.lock().unwrap();
                let points_count = if self.active_tab_note_on {
                    dual_curve.note_on_curve.control_points.len()
                } else {
                    dual_curve.note_off_curve.control_points.len()
                };
                ui.label(format!("📊 Total Points in {} Curve: {}", curve_type, points_count));
                
                // Список точек для активной кривой
                let points = if self.active_tab_note_on {
                    &dual_curve.note_on_curve.control_points
                } else {
                    &dual_curve.note_off_curve.control_points
                };
                
                if !points.is_empty() {
                    egui::ScrollArea::vertical()
                        .max_height(100.0)
                        .show(ui, |ui| {
                            ui.label(format!("📋 {} Curve Point List:", curve_type));
                            for (i, point) in points.iter().enumerate() {
                                let marker = if Some(i) == self.selected_point { "▶ " } else { "• " };
                                let coord_format = if self.shift_pressed {
                                    format!("{:.2}, {:.2}", point.position.0, point.position.1)
                                } else {
                                    format!("{:.1}, {:.1}", point.position.0, point.position.1)
                                };
                                ui.label(format!("{}Point {}: ({})", marker, i, coord_format));
                            }
                        });
                }
            });

            // Правая часть - панели
            ui.vertical(|ui| {
                ui.set_min_width(400.0);
                
                // Добавляем отступ для выравнивания с заголовком графика
                ui.add_space(31.0);
                
                // MIDI панель
                egui::Frame::group(ui.style())
                    .fill(egui::Color32::from_gray(30))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
                    .show(ui, |ui| {
                        self.draw_midi_panel(ui);
                    });
                
                ui.add_space(10.0);
                
                // Панель пресетов
                egui::Frame::group(ui.style())
                    .fill(egui::Color32::from_gray(30))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
                    .show(ui, |ui| {
                        self.draw_presets_panel(ui);
                    });
            });
        });
    }
    
    
    
    fn draw_test_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("🎯 Тест кривой").size(12.0));
        ui.add_space(3.0);
        
        // Используем интерактивный слайдер для тестового значения
        let mut test_velocity = 64; // Начальное значение
        ui.horizontal(|ui| {
            ui.label("Velocity:");
            ui.add(egui::Slider::new(&mut test_velocity, 0..=127).show_value(false));
            ui.label(format!("{}", test_velocity));
        });
        
        let output_velocity = self.process_input_velocity(test_velocity);
        
        ui.add_space(5.0);
        
        // Визуальная индикация
        ui.vertical(|ui| {
            let bar_width = 200.0;
            
            // Полоса входного значения
            ui.horizontal(|ui| {
                ui.label("In:");
                let input_ratio = test_velocity as f32 / 127.0;
                ui.add_sized(
                    [bar_width, 12.0],
                    egui::widgets::ProgressBar::new(input_ratio)
                        .fill(egui::Color32::from_rgb(100, 100, 200))
                );
            });
            
            // Полоса выходного значения
            ui.horizontal(|ui| {
                ui.label("Out:");
                let output_ratio = output_velocity as f32 / 127.0;
                ui.add_sized(
                    [bar_width, 12.0],
                    egui::widgets::ProgressBar::new(output_ratio)
                        .fill(egui::Color32::from_rgb(100, 200, 100))
                );
            });
        });
        
        ui.add_space(3.0);
        
        if ui.button("🧪 Тест").clicked() {
            self.test_curve();
        }
    }
    
    fn draw_presets_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("📁 Presets").size(14.0));
        ui.add_space(5.0);
        
        // Get presets list
        let preset_names = self.preset_manager.lock().unwrap().get_preset_names();
        
        // Show currently selected preset
        if let Some(selected) = &self.selected_preset {
            ui.label(format!("Current: {}", selected));
        } else {
            ui.label("Current: (custom curve)");
        }
        
        ui.add_space(5.0);
        
        // Preset control buttons
        ui.horizontal(|ui| {
            if ui.button("Load").clicked() {
                if !preset_names.is_empty() {
                    self.load_preset(&preset_names[0]);
                }
            }
            
            if ui.button("Save").clicked() {
                // TODO: Show save dialog
            }
            
            if ui.button("Delete").clicked() {
                if let Some(selected_preset) = &self.selected_preset {
                    if let Err(e) = self.preset_manager.lock().unwrap().remove_preset(selected_preset) {
                        eprintln!("Preset deletion error: {}", e);
                    }
                }
            }
        });
        
        ui.add_space(5.0);
        
        // Available presets list with scrolling
        ui.label("Available Presets:");
        egui::ScrollArea::vertical()
            .max_height(120.0)
            .show(ui, |ui| {
                for preset_name in preset_names {
                    let is_selected = self.selected_preset.as_ref() == Some(&preset_name);
                    if ui.selectable_label(is_selected, &preset_name).clicked() {
                        self.selected_preset = Some(preset_name.clone());
                        // Загружаем пресет сразу при клике на название
                        self.load_preset(&preset_name);
                    }
                }
            });
    }
    
    fn draw_midi_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("🎹 MIDI Panel").size(12.0));
        ui.add_space(3.0);
        
        // Hi-Res MIDI toggle
        ui.group(|ui| {
            let mut hi_res_enabled = self.dual_curve.lock().unwrap().is_hi_res_enabled();
            
            if ui.checkbox(&mut hi_res_enabled, "Enable Hi-Res MIDI (14-bit)").clicked() {
                // Обновляем состояние в dual_curve
                self.dual_curve.lock().unwrap().set_hi_res_enabled(hi_res_enabled);
                
                // Обновляем в MIDI менеджере тоже (синхронизация)
                // (dual_curve уже является Arc, поэтому изменения автоматически распространятся)
                
                // Сохраняем в настройки
                self.settings_manager.set_hi_res_enabled(hi_res_enabled);
                self.auto_save_settings();
            }
            
            if hi_res_enabled {
                ui.colored_label(egui::Color32::from_rgb(100, 200, 100), "✓ Hi-Res: 14-bit (0-16383)");
            } else {
                ui.colored_label(egui::Color32::from_rgb(200, 200, 100), "Standard: 7-bit (0-127)");
            }
        });
        
        ui.add_space(10.0);
        
        // MIDI control buttons
        ui.horizontal(|ui| {
            if ui.button("🔄 Refresh").clicked() {
                self.refresh_midi_ports();
            }
            
            if ui.button("🎵 Test").clicked() {
                if let Err(e) = self.send_test_midi_message() {
                    eprintln!("MIDI test error: {}", e);
                }
            }
        });
        
        ui.add_space(10.0);
        
        // MIDI connection status
        let is_active = self.is_midi_active();
        let status_text = format!("Status: {}", if is_active { "Active" } else { "Inactive" });
        let status_color = if is_active {
            egui::Color32::from_rgb(100, 200, 100) // Green
        } else {
            egui::Color32::from_rgb(200, 100, 100) // Red
        };
        
        ui.colored_label(status_color, status_text);
        
        ui.add_space(5.0);
        
        // Input MIDI port selection dropdown
        ui.horizontal(|ui| {
            ui.label("📥 Input Port:");
            
            let selected_text = match &self.selected_input_port {
                Some(name) => name.clone(),
                None => "Not Selected".to_string(),
            };
            
            // Store selected port index for changes
            let mut input_port_changed = None;
            
            egui::ComboBox::from_id_source("input_port_selector")
                .selected_text(&selected_text)
                .show_ui(ui, |ui| {
                    // Disconnect option
                    if ui.selectable_label(self.selected_input_port.is_none(), "🔌 Disconnected").clicked() {
                        input_port_changed = Some(None);
                        ui.close_menu();
                    }
                    
                    // Available ports list
                    for (index, port_name) in self.midi_input_ports.iter().enumerate() {
                        let is_selected = self.selected_input_port.as_ref() == Some(port_name);
                        let display_text = if is_selected { "🔗 " } else { "📥 " };
                        
                        if ui.selectable_label(is_selected, format!("{}{}", display_text, port_name)).clicked() {
                            input_port_changed = Some(Some(index));
                            ui.close_menu();
                        }
                    }
                });
                
            // Handle selection changes after UI display
            if let Some(maybe_index) = input_port_changed {
                if let Some(index) = maybe_index {
                    // Clone port name before method calls
                    if let Some(port_name) = self.midi_input_ports.get(index) {
                        let port_name_cloned = port_name.clone();
                        match self.connect_input_port(&port_name_cloned) {
                            Ok(_) => {
                                self.selected_input_port = Some(port_name_cloned);
                            }
                            Err(e) => {
                                eprintln!("Input port connection error: {}", e);
                            }
                        }
                    }
                } else {
                    // Disconnect port
                    let _ = self.disconnect_input_port();
                }
            }
        });
        
        ui.add_space(3.0);
        
        // Output MIDI port selection dropdown
        ui.horizontal(|ui| {
            ui.label("📤 Output Port:");
            
            let selected_text = match &self.selected_output_port {
                Some(name) => name.clone(),
                None => "Not Selected".to_string(),
            };
            
            // Store selected port index for changes
            let mut output_port_changed = None;
            
            egui::ComboBox::from_id_source("output_port_selector")
                .selected_text(&selected_text)
                .show_ui(ui, |ui| {
                    // Disconnect option
                    if ui.selectable_label(self.selected_output_port.is_none(), "🔌 Disconnected").clicked() {
                        output_port_changed = Some(None);
                        ui.close_menu();
                    }
                    
                    // Available ports list
                    for (index, port_name) in self.midi_output_ports.iter().enumerate() {
                        let is_selected = self.selected_output_port.as_ref() == Some(port_name);
                        let display_text = if is_selected { "🔗 " } else { "📤 " };
                        
                        if ui.selectable_label(is_selected, format!("{}{}", display_text, port_name)).clicked() {
                            output_port_changed = Some(Some(index));
                            ui.close_menu();
                        }
                    }
                });
                
            // Handle selection changes after UI display
            if let Some(maybe_index) = output_port_changed {
                if let Some(index) = maybe_index {
                    // Clone port name before method calls
                    if let Some(port_name) = self.midi_output_ports.get(index) {
                        let port_name_cloned = port_name.clone();
                        match self.connect_output_port(&port_name_cloned) {
                            Ok(_) => {
                                self.selected_output_port = Some(port_name_cloned);
                            }
                            Err(e) => {
                                eprintln!("Output port connection error: {}", e);
                            }
                        }
                    }
                } else {
                    // Disconnect port
                    let _ = self.disconnect_output_port();
                }
            }
        });
        
        ui.add_space(5.0);
        
        // Disconnect all ports button
        if is_active && ui.button("🔌 Disconnect All").clicked() {
            if let Err(e) = self.disconnect_all_ports() {
                eprintln!("Port disconnection error: {}", e);
            }
        }
        
        if self.midi_input_ports.is_empty() && self.midi_output_ports.is_empty() {
            ui.add_space(3.0);
            ui.colored_label(egui::Color32::YELLOW, "⚠️ No MIDI ports found!");
        }
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
        
        // Отслеживание начала перетаскивания
        if !self.is_dragging && response.dragged() && self.selected_point.is_some() {
            self.is_dragging = true;
        }
        
        // Сохранение настроек при отпускании кнопки мыши (drag release)
        if response.drag_released() && self.is_dragging {
            // Мышь отпущена - сохраняем настройки
            self.is_dragging = false;
            self.auto_save_settings();
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
    
    fn draw_curve(&mut self, painter: &Painter, rect: Rect) {
        // Отрисовка сетки
        self.draw_grid(painter, rect);
        
        // Отрисовка кривой Безье с контрольными точками
        self.draw_bezier_curve(painter, rect);
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
    
fn draw_bezier_curve(&mut self, painter: &Painter, rect: Rect) {
        let mut dual_curve = self.dual_curve.lock().unwrap();
        
        let curve = if self.active_tab_note_on {
            &mut dual_curve.note_on_curve
        } else {
            &mut dual_curve.note_off_curve
        };
        
        if curve.control_points.len() < 2 {
            return;
        }
        
        // Строим точки для кривой через интерполяцию
        let mut curve_points = Vec::new();
        
        // Генерируем точки кривой от 0 до 127 (MIDI velocity range)
        for i in 0..=128 {
            let x_input = i as f32;
            let y_output = curve.evaluate(x_input);
            
            // Конвертируем в экранные координаты
            let screen_x = rect.left() + (x_input / 127.0) * rect.width();
            let screen_y = rect.bottom() - (y_output / 127.0) * rect.height();
            
            curve_points.push(Pos2::new(screen_x, screen_y));
        }
        
        // Отрисовка кривой как плавная линия
        if curve_points.len() >= 2 {
            let curve_color = if self.active_tab_note_on {
                egui::Color32::from_rgb(100, 200, 255) // Голубая для NoteOn
            } else {
                egui::Color32::from_rgb(255, 150, 100) // Оранжевая для NoteOff
            };
            
            painter.add(egui::Shape::line(
                curve_points.clone(),
                egui::Stroke::new(3.0, curve_color)
            ));
        }
        
        // Рисуем контрольные точки как управляющие элементы
        for (i, point) in curve.control_points.iter().enumerate() {
            let screen_pos = self.world_to_screen(Pos2::new(point.position.0, point.position.1), rect);
            
            // Цвет точки зависит от выбранности и типа кривой
            let (color, stroke_color) = if Some(i) == self.selected_point {
                if self.active_tab_note_on {
                    (egui::Color32::from_rgb(255, 100, 100), egui::Color32::RED) // Красный для выбранной NoteOn
                } else {
                    (egui::Color32::from_rgb(255, 150, 50), egui::Color32::from_rgb(255, 100, 0)) // Оранжевый для выбранной NoteOff
                }
            } else {
                if self.active_tab_note_on {
                    (egui::Color32::from_rgb(255, 150, 150), egui::Color32::from_rgb(200, 100, 100)) // Розовый для NoteOn
                } else {
                    (egui::Color32::from_rgb(255, 180, 120), egui::Color32::from_rgb(200, 120, 50)) // Светло-оранжевый для NoteOff
                }
            };
            
            // Рисуем точку
            painter.circle_filled(screen_pos, 6.0, color);
            painter.circle_stroke(screen_pos, 6.0, egui::Stroke::new(1.0, stroke_color));
            
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
    
// Метод draw_control_points удален - отрисовка точек интегрирована в draw_bezier_curve
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 600.0])
            .with_title("MIDI Curves Plugin - Beta")
            .with_resizable(true)
            .with_fullscreen(false)
            .with_decorations(true),
        ..Default::default()
    };
    
    let app = MidiCurvesApp::new().unwrap();
    
    eframe::run_native(
        "MIDI Curves Plugin",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(app))
        }),
    )
}