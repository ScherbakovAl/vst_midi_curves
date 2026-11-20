//! MIDI Curves — standalone: вход MIDI → кривые velocity → выход MIDI.
//!
//! Copyright (c) 2025 Scherbakov Alexey <scherbakov.al@gmail.com>
//! Licensed under the MIT License
//!
//! Потоки: колбэк midir — весь путь MIDI (см. `midi.rs`); поток eframe — GUI;
//! поток-наблюдатель будит GUI, когда сыграна нота (маркер на кривой): сам
//! поток MIDI egui не трогает; поток `midi-ports` раз в [`midi::POLL_PERIOD`]
//! обновляет списки портов и автоподключает выбранные (работает и при
//! свёрнутом окне, когда eframe не зовёт `update`).

mod midi;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use eframe::egui;
use vst_midi_curves::curve::{CurveBank, CurveSet, Slot};
use vst_midi_curves::editor::CurvesUi;
use vst_midi_curves::library::{self, Library};
use vst_midi_curves::settings::{self, Settings};

struct App {
    bank: Arc<CurveBank>,
    set: CurveSet,
    ui: CurvesUi,
    /// Общее с потоком `midi-ports`; путь MIDI (колбэк midir) мьютекс не берёт.
    midi: Arc<Mutex<midi::Midi>>,
    settings: Settings,
    settings_path: Option<PathBuf>,
    /// Ошибка настроек для строки состояния.
    status: Option<String>,
}

fn lock(m: &Mutex<midi::Midi>) -> MutexGuard<'_, midi::Midi> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl App {
    fn new() -> Self {
        let settings_path = settings::default_path();
        let (settings, err) = match &settings_path {
            Some(p) => Settings::load(p),
            None => (Settings::default(), Some("no config directory: settings are not saved".into())),
        };
        let bank = Arc::new(CurveBank::new());
        let set = settings.curves.clone();
        set.apply(&bank);
        let tab = if settings.active_curve_tab == 1 { Slot::NoteOff } else { Slot::NoteOn };
        // Запомненные порты подключаются сразу, если они есть; нет — их
        // подключит поток `midi-ports`, когда появятся.
        let mut midi = midi::Midi::new(bank.clone());
        midi.select(settings.last_input_port.clone(), settings.last_output_port.clone());
        Self {
            ui: CurvesUi::new(&set, tab, Library::new(library::default_dir())),
            midi: Arc::new(Mutex::new(midi)),
            bank,
            set,
            settings,
            settings_path,
            status: err,
        }
    }

    fn save(&mut self) {
        self.settings.curves = self.set.clone();
        self.settings.active_curve_tab = self.ui.tab.idx() as u8;
        if let Some(p) = &self.settings_path {
            if let Err(e) = self.settings.save(p) {
                self.status = Some(format!("settings: {e}"));
            }
        }
    }
}

/// Список портов в выпадающем меню; `Some(выбор)` — пользователь выбрал.
fn port_combo(ui: &mut egui::Ui, id: &str, current: &Option<String>, ports: &[String]) -> Option<Option<String>> {
    let mut pick = None;
    let text = current.as_deref().unwrap_or("— none —");
    egui::ComboBox::from_id_salt(id).width(190.0).selected_text(text).show_ui(ui, |ui| {
        if ui.selectable_label(current.is_none(), "— none —").clicked() {
            pick = Some(None);
        }
        for p in ports {
            if ui.selectable_label(current.as_deref() == Some(p), p).clicked() {
                pick = Some(Some(p.clone()));
            }
        }
    });
    pick
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut pick_in = None;
        let mut pick_out = None;
        let mut refresh = false;
        let mut midi = lock(&self.midi);
        let (settings, m, status) = (&self.settings, &*midi, &self.status);
        let out = self.ui.show(ctx, &mut self.set, &self.bank, |ui| {
            ui.label(egui::RichText::new("MIDI").strong());
            ui.horizontal(|ui| {
                ui.label("In ");
                pick_in = port_combo(ui, "midi_in", &settings.last_input_port, &m.ports.inputs);
            });
            ui.horizontal(|ui| {
                ui.label("Out");
                pick_out = port_combo(ui, "midi_out", &settings.last_output_port, &m.ports.outputs);
            });
            ui.horizontal(|ui| {
                refresh = ui
                    .button("Refresh")
                    .on_hover_text("Re-scan MIDI ports and reconnect (ports are also re-scanned every 2 s)")
                    .clicked();
                let (c, t) = match (&m.input, &m.output) {
                    (Some(_), Some(_)) => (egui::Color32::from_rgb(100, 200, 100), "connected"),
                    (Some(_), None) => (egui::Color32::from_rgb(220, 200, 100), "input only"),
                    _ => (egui::Color32::from_rgb(200, 110, 100), "not connected"),
                };
                ui.colored_label(c, t);
            });
            for text in [&m.error, status].into_iter().flatten() {
                ui.colored_label(egui::Color32::from_rgb(230, 110, 100), text);
            }
            ui.separator();
        });

        for slot in Slot::ALL {
            if out.shape[slot.idx()] {
                self.bank.store(slot, self.set.shape(slot));
            }
        }
        if out.hi_res {
            self.bank.set_hi_res(self.set.hi_res);
        }
        let picked = pick_in.is_some() || pick_out.is_some();
        if let Some(p) = pick_in {
            self.settings.last_input_port = p;
        }
        if let Some(p) = pick_out {
            self.settings.last_output_port = p;
        }
        // Refresh переподключает и исправное на вид соединение: устройство,
        // переподключённое быстрее периода опроса, возвращается под тем же
        // адресом ALSA, и по списку портов его прежняя подписка не отличима.
        if refresh || picked {
            midi.select(self.settings.last_input_port.clone(), self.settings.last_output_port.clone());
        }
        drop(midi);
        if out.committed || out.hi_res || out.tab || picked {
            self.save();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save();
    }
}

/// Будит GUI, когда поток MIDI обработал ноту (маркер), не чаще 30 Гц. Поток
/// MIDI сам `request_repaint()` не зовёт: это блокировка контекста egui.
fn spawn_repaint_watcher(ctx: egui::Context, bank: Arc<CurveBank>) {
    let _ = std::thread::Builder::new().name("repaint-watcher".into()).spawn(move || {
        let mut seen = bank.notes();
        loop {
            std::thread::sleep(Duration::from_millis(33));
            let n = bank.notes();
            if n != seen {
                seen = n;
                ctx.request_repaint();
            }
        }
    });
}

/// Раз в [`midi::POLL_PERIOD`]: свежие списки портов и автоподключение
/// выбранных; GUI будится, только если что-то изменилось.
fn spawn_port_watcher(ctx: egui::Context, midi: Arc<Mutex<midi::Midi>>) {
    let _ = std::thread::Builder::new().name("midi-ports".into()).spawn(move || loop {
        std::thread::sleep(midi::POLL_PERIOD);
        if lock(&midi).refresh() {
            ctx.request_repaint();
        }
    });
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 640.0]).with_title("MIDI Curves"),
        ..Default::default()
    };
    let app = App::new();
    let (bank, midi) = (app.bank.clone(), app.midi.clone());
    eframe::run_native(
        "MIDI Curves",
        options,
        Box::new(move |cc| {
            spawn_repaint_watcher(cc.egui_ctx.clone(), bank);
            spawn_port_watcher(cc.egui_ctx.clone(), midi);
            Ok(Box::new(app))
        }),
    )
}
