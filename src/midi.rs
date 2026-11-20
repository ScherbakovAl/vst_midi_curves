//! MIDI standalone через midir (ALSA на Linux).
//!
//! Весь путь MIDI — в колбэке входа midir, в его потоке: разбор, кривая
//! ([`velocity::process_midi`]) и отправка на выход в том же вызове, без
//! очередей, блокировок и аллокаций. Выходное соединение и префикс CC88
//! принадлежат замыканию колбэка (GUI их не трогает); смена любого порта
//! пересоздаёт соединение целиком.
//!
//! Устройства: [`Midi`] помнит выбранную пару портов (цель) и раз в
//! [`POLL_PERIOD`] сверяет её со списком портов системы ([`Midi::refresh`],
//! поток `midi-ports` в `main.rs`) — как `MidiManager::poll` в piano_sim:
//! пропавший порт закрывает соединение, появившийся (в том числе впервые
//! после запуска, под другим номером клиента ALSA) — подключает.

use std::sync::Arc;
use std::time::Duration;

use midir::{Ignore, MidiIO, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

use vst_midi_curves::curve::CurveBank;
use vst_midi_curves::velocity::{self, Prefix};

const CLIENT: &str = "MIDI Curves";

/// Период опроса списка портов (как `MIDI_POLL_PERIOD` в piano_sim).
pub const POLL_PERIOD: Duration = Duration::from_secs(2);

/// Состояние, которым владеет колбэк входа.
struct Route {
    out: Option<MidiOutputConnection>,
    prefix: Prefix,
}

/// Списки портов системы на момент опроса.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ports {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// Порт этого же приложения («MIDI Curves:midi-curves-in …»): свой вход,
/// выбранный выходом, — петля, поэтому в списки он не попадает.
fn is_own(name: &str) -> bool {
    name.strip_prefix(CLIENT).is_some_and(|rest| rest.starts_with(':'))
}

/// Клиенты ALSA для опроса портов, живут всё время работы: новый клиент на
/// каждый опрос — лишние события появления/ухода клиента у всех слушателей
/// (PipeWire и т. п.).
struct Scanner {
    input: Option<MidiInput>,
    output: Option<MidiOutput>,
}

impl Scanner {
    fn new() -> Self {
        Self { input: MidiInput::new(CLIENT).ok(), output: MidiOutput::new(CLIENT).ok() }
    }

    fn scan(&self) -> Ports {
        Ports { inputs: names(self.input.as_ref()), outputs: names(self.output.as_ref()) }
    }
}

/// Имена портов сейчас (свежий опрос системы), без своих.
fn names<M: MidiIO>(m: Option<&M>) -> Vec<String> {
    m.map_or_else(Vec::new, |m| m.ports().iter().filter_map(|p| m.port_name(p).ok()).filter(|n| !is_own(n)).collect())
}

/// Имя без адреса ALSA « 24:0» в конце: номер клиента меняется при
/// переподключении устройства, имя — нет.
fn stable_name(name: &str) -> &str {
    match name.rsplit_once(' ') {
        Some((head, addr))
            if addr.split_once(':').is_some_and(|(c, p)| {
                !c.is_empty() && !p.is_empty() && c.bytes().chain(p.bytes()).all(|b| b.is_ascii_digit())
            }) =>
        {
            head
        }
        _ => name,
    }
}

/// Индекс порта: точное совпадение имени, иначе единственное совпадение без
/// адреса ALSA. Никаких «первый попавшийся».
pub fn resolve(names: &[String], wanted: &str) -> Option<usize> {
    if let Some(i) = names.iter().position(|n| n == wanted) {
        return Some(i);
    }
    let w = stable_name(wanted);
    let mut hits = names.iter().enumerate().filter(|(_, n)| stable_name(n) == w);
    match (hits.next(), hits.next()) {
        (Some((i, _)), None) => Some(i),
        _ => None,
    }
}

/// Подключение: вход (колбэк) → кривые → выход.
pub struct Midi {
    bank: Arc<CurveBank>,
    scanner: Scanner,
    conn: Option<MidiInputConnection<Route>>,
    /// Выбор пользователя (цель автоподключения), как в настройках.
    wanted_in: Option<String>,
    wanted_out: Option<String>,
    /// Фактически подключённые порты.
    pub input: Option<String>,
    pub output: Option<String>,
    /// Порты системы по последнему опросу (для выпадающих списков).
    pub ports: Ports,
    /// Почему цель не подключена: ошибка подключения или ожидание устройства.
    pub error: Option<String>,
}

impl Midi {
    pub fn new(bank: Arc<CurveBank>) -> Self {
        Self {
            bank,
            scanner: Scanner::new(),
            conn: None,
            wanted_in: None,
            wanted_out: None,
            input: None,
            output: None,
            ports: Ports::default(),
            error: None,
        }
    }

    /// Выбор пользователя: новая цель, подключение сразу (если порты есть).
    pub fn select(&mut self, input: Option<String>, output: Option<String>) {
        self.wanted_in = input;
        self.wanted_out = output;
        self.close();
        self.refresh();
    }

    /// Опросить порты и привести подключение к цели. `true` — изменилось
    /// что-то видимое в GUI (списки, подключение, сообщение).
    pub fn refresh(&mut self) -> bool {
        let before = (self.ports.clone(), self.input.clone(), self.output.clone(), self.error.clone());
        self.ports = self.scanner.scan();
        self.sync();
        before != (self.ports.clone(), self.input.clone(), self.output.clone(), self.error.clone())
    }

    /// Привести подключение к цели по списку `self.ports`. Исправное
    /// соединение не трогается.
    fn sync(&mut self) {
        let Some(wi) = self.wanted_in.clone() else {
            // Без входа нечего обрабатывать (см. `connect`).
            self.close();
            self.error = None;
            return;
        };
        let alive = |cur: &Option<String>, list: &[String]| cur.as_ref().is_some_and(|c| list.contains(c));
        let in_ok = alive(&self.input, &self.ports.inputs);
        let out_ok = self.wanted_out.is_none() || alive(&self.output, &self.ports.outputs);
        if self.conn.is_some() && in_ok && out_ok {
            return;
        }
        let wo = self.wanted_out.clone();
        let mut missing = Vec::new();
        if resolve(&self.ports.inputs, &wi).is_none() {
            missing.push(stable_name(&wi));
        }
        if let Some(o) = wo.as_deref().filter(|o| resolve(&self.ports.outputs, o).is_none()) {
            missing.push(stable_name(o));
        }
        if missing.is_empty() {
            self.error = self.connect(Some(&wi), wo.as_deref()).err();
        } else {
            // Устройство пропало (уснуло, выдернуто) или ещё не появилось.
            self.close();
            self.error = Some(format!("waiting for «{}»", missing.join("», «")));
        }
    }

    fn close(&mut self) {
        if let Some(c) = self.conn.take() {
            c.close();
        }
        self.input = None;
        self.output = None;
    }

    /// Подключить пару портов (любой может быть `None`). Сначала закрывается
    /// прежнее соединение (и вход, и выход). При ошибке не подключено ничего.
    fn connect(&mut self, input: Option<&str>, output: Option<&str>) -> Result<(), String> {
        self.close();
        let out = match output {
            Some(name) => {
                let m = MidiOutput::new(CLIENT).map_err(|e| e.to_string())?;
                let ports = m.ports();
                let names: Vec<String> = ports.iter().filter_map(|p| m.port_name(p).ok()).collect();
                let i = resolve(&names, name).ok_or_else(|| format!("output «{name}» not found"))?;
                let conn = m.connect(&ports[i], "midi-curves-out").map_err(|e| e.to_string())?;
                Some((conn, names[i].clone()))
            }
            None => None,
        };
        let Some(name) = input else {
            // Без входа нечего обрабатывать; выход закроется вместе с `out`.
            return Ok(());
        };
        let mut m = MidiInput::new(CLIENT).map_err(|e| e.to_string())?;
        // Пропускать дальше всё: SysEx, clock, active sensing.
        m.ignore(Ignore::None);
        let ports = m.ports();
        let names: Vec<String> = ports.iter().filter_map(|p| m.port_name(p).ok()).collect();
        let i = resolve(&names, name).ok_or_else(|| format!("input «{name}» not found"))?;
        let (out, out_name) = match out {
            Some((c, n)) => (Some(c), Some(n)),
            None => (None, None),
        };
        let bank = self.bank.clone();
        let conn = m
            .connect(
                &ports[i],
                "midi-curves-in",
                move |_stamp, msg, route: &mut Route| {
                    velocity::process_midi(&bank, &mut route.prefix, msg, |m| {
                        if let Some(o) = route.out.as_mut() {
                            let _ = o.send(m);
                        }
                    });
                },
                Route { out, prefix: Prefix::new() },
            )
            .map_err(|e| e.to_string())?;
        self.conn = Some(conn);
        self.input = Some(names[i].clone());
        self.output = out_name;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_names_resolve_exactly_or_by_stable_name() {
        let names: Vec<String> = ["Midi Through:Midi Through Port-0 14:0", "TinyUSB:TinyUSB MIDI 1 24:0", "Other 1"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(resolve(&names, "Midi Through:Midi Through Port-0 14:0"), Some(0));
        // Устройство переподключилось под другим номером клиента.
        assert_eq!(resolve(&names, "TinyUSB:TinyUSB MIDI 1 28:0"), Some(1));
        // Не похоже ни на один — не подключаем «первый попавшийся».
        assert_eq!(resolve(&names, "Keystation"), None);
        assert_eq!(resolve(&names, "TinyUSB"), None);
        assert_eq!(stable_name("Other 1"), "Other 1");
        // Два одинаковых устройства — неоднозначно, без угадывания.
        let twins: Vec<String> = vec!["K:K 1 20:0".into(), "K:K 1 21:0".into()];
        assert_eq!(resolve(&twins, "K:K 1 22:0"), None);
        assert!(is_own("MIDI Curves:midi-curves-in 130:0"));
        assert!(!is_own("MIDI Curves Pro:in 131:0"));
    }

    /// Горячее подключение на виртуальных портах ALSA: «клавиатура» и
    /// «синтезатор» появляются после выбора, пропадают и возвращаются под
    /// другими номерами клиента; ноты каждый раз доходят насквозь.
    #[test]
    #[cfg(unix)] // виртуальные порты midir — только ALSA/CoreMIDI
    fn hotplug_waits_and_reconnects() {
        use midir::os::unix::{VirtualInput, VirtualOutput};
        use std::sync::mpsc;
        if MidiInput::new("probe").is_err() {
            return; // нет секвенсера MIDI — проверять нечего
        }
        let kbd = || MidiOutput::new("hp-kbd").unwrap().create_virtual("keys").unwrap();
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let synth = || {
            let tx = tx.clone();
            let cb = move |_: u64, m: &[u8], _: &mut ()| drop(tx.send(m.to_vec()));
            MidiInput::new("hp-synth").unwrap().create_virtual("in", cb, ()).unwrap()
        };
        let note_passes = |k: &mut MidiOutputConnection| {
            while rx.try_recv().is_ok() {}
            k.send(&[0x90, 60, 100]).unwrap();
            let t = std::time::Instant::now() + Duration::from_secs(2);
            while let Ok(m) = rx.recv_timeout(t.saturating_duration_since(std::time::Instant::now())) {
                if m[0] == 0x90 && m[1] == 60 {
                    return true;
                }
            }
            false
        };
        let mut midi = Midi::new(Arc::new(CurveBank::new()));
        // Запомненные имена с устаревшими адресами; устройств ещё нет.
        midi.select(Some("hp-kbd:keys 250:0".into()), Some("hp-synth:in 251:0".into()));
        assert!(midi.input.is_none());
        assert_eq!(midi.error.as_deref(), Some("waiting for «hp-kbd:keys», «hp-synth:in»"));

        let mut k = kbd();
        assert!(midi.refresh());
        assert_eq!(midi.error.as_deref(), Some("waiting for «hp-synth:in»"));
        let s = synth();
        assert!(midi.refresh());
        assert!(midi.input.as_deref().is_some_and(|n| n.starts_with("hp-kbd:keys ")), "{:?}", midi.error);
        assert!(midi.output.as_deref().is_some_and(|n| n.starts_with("hp-synth:in ")));
        assert!(midi.error.is_none());
        assert!(note_passes(&mut k));
        // Свои порты в списках не показываются.
        assert!(!midi.ports.inputs.iter().chain(&midi.ports.outputs).any(|n| is_own(n)));
        // Исправное соединение опрос не трогает.
        assert!(!midi.refresh());

        // Клавиатура пропала и вернулась под другим номером клиента (ALSA
        // выдаёт наименьший свободный — прежний номер займёт заглушка).
        let old = midi.input.clone();
        drop(k);
        assert!(midi.refresh());
        assert!(midi.input.is_none());
        assert_eq!(midi.error.as_deref(), Some("waiting for «hp-kbd:keys»"));
        let _filler = MidiOutput::new("hp-filler").unwrap();
        let mut k = kbd();
        assert!(midi.refresh());
        assert!(midi.input.is_some() && midi.input != old);
        assert!(note_passes(&mut k));

        // То же с выходом.
        drop(s);
        assert!(midi.refresh());
        assert!(midi.input.is_none() && midi.output.is_none());
        let _s = synth();
        assert!(midi.refresh());
        assert!(note_passes(&mut k));

        // Выбор «нет входа» — ничего не подключено и не ждём.
        midi.select(None, Some("hp-synth:in 251:0".into()));
        assert!(midi.input.is_none() && midi.output.is_none() && midi.error.is_none());
    }
}
