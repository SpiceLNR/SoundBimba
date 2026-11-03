// SPDX-License-Identifier: AGPL-3.0-or-later
//! «Оригинал звука» программы (этап 7): куда Windows направляет звук программы, если человек
//! выбрал в узле «На устройство» или «Только через SoundBimba». Меняется настройка Windows
//! (недокументированный `winaudio::routing`), поэтому:
//! - каждое изменение записывается в журнал (`routing.json`) вместе с прежним значением;
//! - когда выбор снят, узел удалён, звук выключен или SoundBimba закрывается — прежнее значение
//!   возвращается;
//! - Windows принимает настройку только у процесса, у которого уже есть звук, — поэтому всё
//!   применяется и возвращается, когда программа «зазвучит» (проверка раз в 2 с), а пока её
//!   нет — ждёт в журнале (в том числе до следующего запуска SoundBimba).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Что SoundBimba поменяла для программы (ключ журнала — имя exe в нижнем регистре).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Куда направлено сейчас (id устройства).
    pub device: String,
    /// Что было до SoundBimba: None — «по умолчанию».
    pub previous: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct Journal {
    pub entries: BTreeMap<String, Entry>,
}

impl Journal {
    pub fn load(path: &Path) -> Journal {
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        crate::storage::write_atomic(path, &serde_json::to_string_pretty(self).unwrap_or_default())
    }
}

/// Состояние перенаправления для узла (по программе).
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RouteState {
    /// Настройка Windows выставлена.
    Applied,
    /// Программа ещё не издаёт звук — выставим, как только зазвучит.
    Waiting,
    /// Windows отказала (подробность — в журнале программы).
    Error,
}

/// Доступ к Windows (настоящий — `WindowsRoutes`, в тестах — подделка).
pub trait Routes {
    /// Процессы со звуком: (exe, PID).
    fn sessions(&self) -> Vec<(String, u32)>;
    fn get(&self, pid: u32) -> Result<Option<String>, String>;
    fn set(&self, pid: u32, device: Option<&str>) -> Result<(), String>;
}

pub struct WindowsRoutes;

impl Routes for WindowsRoutes {
    fn sessions(&self) -> Vec<(String, u32)> {
        winaudio::processes::audio_sessions().unwrap_or_default()
    }
    fn get(&self, pid: u32) -> Result<Option<String>, String> {
        winaudio::routing::output_of(pid).map_err(|e| e.message())
    }
    fn set(&self, pid: u32, device: Option<&str>) -> Result<(), String> {
        winaudio::routing::set_output_of(pid, device).map_err(|e| e.message())
    }
}

/// Приводит Windows к желаемому: `desired` — exe (нижний регистр) → устройство. Всё, что есть в
/// журнале, но больше не желаемо, возвращается к прежнему. Возвращает состояние по программам и
/// изменился ли журнал (тогда его надо сохранить).
pub fn sync(desired: &BTreeMap<String, String>, journal: &mut Journal, sys: &dyn Routes) -> (HashMap<String, RouteState>, bool) {
    let mut by_exe: HashMap<String, Vec<u32>> = HashMap::new();
    for (exe, pid) in sys.sessions() {
        by_exe.entry(exe.to_lowercase()).or_default().push(pid);
    }
    let mut states = HashMap::new();
    let mut changed = false;

    for (exe, device) in desired {
        let pids = by_exe.get(exe).cloned().unwrap_or_default();
        if pids.is_empty() {
            // Программа молчит: запомним желаемое, выставим позже. Прежнее значение узнаем тогда же.
            states.insert(exe.clone(), RouteState::Waiting);
            continue;
        }
        let mut state = RouteState::Applied;
        for pid in pids {
            let current = match sys.get(pid) {
                Ok(c) => c,
                Err(e) => {
                    crate::applog::error(&format!("routing: cannot read output of {exe} ({pid}): {e}"));
                    state = RouteState::Error;
                    continue;
                }
            };
            if !journal.entries.contains_key(exe) {
                journal.entries.insert(exe.clone(), Entry { device: device.clone(), previous: current.clone() });
                changed = true;
            }
            let entry = journal.entries.get_mut(exe).unwrap();
            if entry.device != *device {
                entry.device = device.clone();
                changed = true;
            }
            if current.as_deref() != Some(device.as_str()) {
                match sys.set(pid, Some(device)) {
                    Ok(()) => crate::applog::info(&format!("routing: {exe} ({pid}) -> {device}")),
                    Err(e) => {
                        crate::applog::error(&format!("routing: cannot redirect {exe} ({pid}): {e}"));
                        state = RouteState::Error;
                    }
                }
            }
        }
        states.insert(exe.clone(), state);
    }

    // Больше не нужно — вернуть прежнее (если программа сейчас звучит; иначе ждём её в журнале).
    let stale: Vec<String> = journal.entries.keys().filter(|e| !desired.contains_key(*e)).cloned().collect();
    for exe in stale {
        let Some(pids) = by_exe.get(&exe) else { continue };
        let previous = journal.entries[&exe].previous.clone();
        let mut ok = true;
        for &pid in pids {
            match sys.set(pid, previous.as_deref()) {
                Ok(()) => crate::applog::info(&format!("routing: {exe} ({pid}) restored to {}", previous.as_deref().unwrap_or("default"))),
                Err(e) => {
                    crate::applog::error(&format!("routing: cannot restore {exe} ({pid}): {e}"));
                    ok = false;
                }
            }
        }
        if ok {
            journal.entries.remove(&exe);
            changed = true;
        }
    }
    (states, changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Подделка Windows: сессии и настройка по PID.
    #[derive(Default)]
    struct Fake {
        sessions: Vec<(String, u32)>,
        outputs: RefCell<HashMap<u32, Option<String>>>,
        fail_set: bool,
    }

    impl Routes for Fake {
        fn sessions(&self) -> Vec<(String, u32)> {
            self.sessions.clone()
        }
        fn get(&self, pid: u32) -> Result<Option<String>, String> {
            Ok(self.outputs.borrow().get(&pid).cloned().flatten())
        }
        fn set(&self, pid: u32, device: Option<&str>) -> Result<(), String> {
            if self.fail_set {
                return Err("E_INVALIDARG".into());
            }
            self.outputs.borrow_mut().insert(pid, device.map(String::from));
            Ok(())
        }
    }

    fn want(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn redirects_every_process_of_the_program_and_remembers_previous() {
        let sys = Fake { sessions: vec![("chrome.exe".into(), 10), ("Chrome.exe".into(), 11), ("game.exe".into(), 20)], ..Default::default() };
        sys.outputs.borrow_mut().insert(10, Some("user-choice".into()));
        let mut j = Journal::default();
        let (st, changed) = sync(&want(&[("chrome.exe", "phones")]), &mut j, &sys);
        assert!(changed);
        assert_eq!(st["chrome.exe"], RouteState::Applied);
        assert_eq!(sys.get(10).unwrap().as_deref(), Some("phones"));
        assert_eq!(sys.get(11).unwrap().as_deref(), Some("phones"));
        assert_eq!(sys.get(20).unwrap(), None, "другие программы не тронуты");
        // Прежнее — то, что человек выставил сам до нас.
        assert_eq!(j.entries["chrome.exe"].previous.as_deref(), Some("user-choice"));
        // Повторный вызов ничего не меняет.
        let (_, changed) = sync(&want(&[("chrome.exe", "phones")]), &mut j, &sys);
        assert!(!changed);
    }

    #[test]
    fn restores_previous_when_no_longer_wanted() {
        let sys = Fake { sessions: vec![("chrome.exe".into(), 10)], ..Default::default() };
        sys.outputs.borrow_mut().insert(10, Some("user-choice".into()));
        let mut j = Journal::default();
        sync(&want(&[("chrome.exe", "phones")]), &mut j, &sys);
        let (_, changed) = sync(&want(&[]), &mut j, &sys);
        assert!(changed);
        assert_eq!(sys.get(10).unwrap().as_deref(), Some("user-choice"));
        assert!(j.entries.is_empty());
    }

    #[test]
    fn silent_program_waits_and_pending_restore_survives() {
        // Программа молчит — ждём; журнал не трогаем, пока не узнаем прежнее значение.
        let quiet = Fake::default();
        let mut j = Journal::default();
        let (st, changed) = sync(&want(&[("game.exe", "phones")]), &mut j, &quiet);
        assert_eq!(st["game.exe"], RouteState::Waiting);
        assert!(!changed && j.entries.is_empty());

        // Перенаправили, потом программа закрылась, а выбор сняли: возврат ждёт её появления.
        let playing = Fake { sessions: vec![("game.exe".into(), 7)], ..Default::default() };
        sync(&want(&[("game.exe", "phones")]), &mut j, &playing);
        sync(&want(&[]), &mut j, &quiet);
        assert!(j.entries.contains_key("game.exe"), "вернуть пока некому — запись остаётся");
        let back = Fake { sessions: vec![("game.exe".into(), 8)], ..Default::default() };
        back.outputs.borrow_mut().insert(8, Some("phones".into()));
        sync(&want(&[]), &mut j, &back);
        assert_eq!(back.get(8).unwrap(), None, "вернулось «по умолчанию»");
        assert!(j.entries.is_empty());
    }

    #[test]
    fn windows_refusal_is_reported_and_restore_is_retried() {
        let sys = Fake { sessions: vec![("game.exe".into(), 7)], fail_set: true, ..Default::default() };
        let mut j = Journal::default();
        let (st, _) = sync(&want(&[("game.exe", "phones")]), &mut j, &sys);
        assert_eq!(st["game.exe"], RouteState::Error);
        // Запись в журнале есть — откат попробуем снова; при ошибке отката она не пропадает.
        sync(&want(&[]), &mut j, &sys);
        assert!(j.entries.contains_key("game.exe"));
    }

    #[test]
    fn journal_round_trip() {
        let dir = std::env::temp_dir().join(format!("sb-routes-{}", std::process::id()));
        let path = dir.join("routing.json");
        let mut j = Journal::default();
        j.entries.insert("game.exe".into(), Entry { device: "phones".into(), previous: None });
        j.save(&path).unwrap();
        assert_eq!(Journal::load(&path), j);
        assert_eq!(Journal::load(&dir.join("missing.json")), Journal::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
