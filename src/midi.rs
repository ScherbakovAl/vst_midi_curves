//! MIDI standalone через midir (ALSA на Linux).
//!
//! Весь путь MIDI — в колбэке входа midir, в его потоке: разбор, кривая
//! ([`velocity::process_midi`]) и отправка на выход в том же вызове, без
//! очередей, блокировок и аллокаций. Выходное соединение и префикс CC88
//! принадлежат замыканию колбэка (GUI их не трогает); смена любого порта
//! пересоздаёт соединение целиком.

use std::sync::Arc;

use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

use vst_midi_curves::curve::CurveBank;
use vst_midi_curves::velocity::{self, Prefix};

const CLIENT: &str = "MIDI Curves";

/// Состояние, которым владеет колбэк входа.
struct Route {
    out: Option<MidiOutputConnection>,
    prefix: Prefix,
}

/// Имена портов сейчас (свежий опрос системы).
pub fn input_ports() -> Vec<String> {
    MidiInput::new(CLIENT)
        .map(|m| m.ports().iter().filter_map(|p| m.port_name(p).ok()).collect())
        .unwrap_or_default()
}

pub fn output_ports() -> Vec<String> {
    MidiOutput::new(CLIENT)
        .map(|m| m.ports().iter().filter_map(|p| m.port_name(p).ok()).collect())
        .unwrap_or_default()
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
    conn: Option<MidiInputConnection<Route>>,
    /// Фактически подключённые порты.
    pub input: Option<String>,
    pub output: Option<String>,
}

impl Midi {
    pub fn new(bank: Arc<CurveBank>) -> Self {
        Self { bank, conn: None, input: None, output: None }
    }

    /// Подключить пару портов (любой может быть `None`). Сначала закрывается
    /// прежнее соединение (и вход, и выход). При ошибке не подключено ничего.
    pub fn connect(&mut self, input: Option<&str>, output: Option<&str>) -> Result<(), String> {
        if let Some(c) = self.conn.take() {
            c.close();
        }
        self.input = None;
        self.output = None;
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
    }
}
