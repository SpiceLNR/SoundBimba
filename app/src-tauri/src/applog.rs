// SPDX-License-Identifier: AGPL-3.0-or-later
//! Простой журнал в файл %APPDATA%\SoundBimba\logs\soundbimba.log. Без секретов и личных данных.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const MAX_SIZE: u64 = 1024 * 1024;

static LOG: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn init(path: PathBuf) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    // Старый журнал больше 1 МБ — откладываем в .old, чтобы файл не рос бесконечно.
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_SIZE) {
        let _ = fs::rename(&path, path.with_extension("log.old"));
    }
    *LOG.lock().unwrap() = Some(path);
}

pub fn write(level: &str, message: &str) {
    let line = format!("{} [{level}] {message}\r\n", winaudio::system::local_time());
    if cfg!(debug_assertions) {
        eprint!("{line}");
    }
    let guard = LOG.lock().unwrap();
    if let Some(path) = guard.as_ref() {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

pub fn info(message: &str) {
    write("INFO", message);
}

pub fn error(message: &str) {
    write("ERROR", message);
}
