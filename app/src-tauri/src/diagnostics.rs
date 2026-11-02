// SPDX-License-Identifier: AGPL-3.0-or-later
//! «Сохранить диагностику»: один zip-файл, который пользователь может отправить разработчику.
//! Внутри: отчёт (версии, Windows, WebView2, устройства, виртуальный кабель), проект, настройки,
//! журнал. Секретов и личных файлов нет. Дополняется по мере роста приложения.

use std::fmt::Write as _;
use std::fs;

use crate::storage::Paths;
use crate::zip;

pub fn report(paths: &Paths, ui_info: &str, audio: &str) -> String {
    let mut r = String::new();
    let now = winaudio::system::local_time();
    let _ = writeln!(r, "SoundBimba diagnostics");
    let _ = writeln!(r, "time: {now}");
    let _ = writeln!(r, "app version: {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(r, "windows: {}", winaudio::system::windows_info().summary());
    let _ = writeln!(
        r,
        "webview2 (registry): {}",
        winaudio::system::webview2_version().unwrap_or_else(|| "not found".into())
    );
    let _ = writeln!(
        r,
        "webview2 (runtime): {}",
        tauri::webview_version().unwrap_or_else(|e| format!("unknown ({e})"))
    );
    let _ = writeln!(r, "data folder: {}", paths.dir.display());
    let _ = writeln!(r, "ui: {ui_info}");

    let _ = writeln!(r, "\naudio devices:");
    match winaudio::devices::list() {
        Ok(devs) => {
            for d in &devs {
                let _ = writeln!(
                    r,
                    "  [{}]{} {} | adapter: {}{}",
                    if d.render { "out" } else { "in " },
                    if d.is_default { " [default]" } else { "" },
                    d.name,
                    d.adapter,
                    d.cable.map(|c| format!(" | virtual cable: {}", c.label)).unwrap_or_default()
                );
            }
            let cables: Vec<_> = devs.iter().filter_map(|d| d.cable.map(|c| c.label)).collect();
            if cables.is_empty() {
                let _ = writeln!(r, "virtual cable: NOT FOUND");
            } else {
                let mut cables = cables;
                cables.dedup();
                let _ = writeln!(r, "virtual cable: {}", cables.join(", "));
            }
        }
        Err(e) => {
            let _ = writeln!(r, "  error: {} (0x{:08X})", e.message().trim(), e.code().0 as u32);
        }
    }
    let _ = writeln!(r, "\naudio engine:\n{audio}");
    r
}

pub fn save(paths: &Paths, target: &str, ui_info: &str, audio: &str) -> std::io::Result<()> {
    let mut files = vec![("report.txt".to_string(), report(paths, ui_info, audio).into_bytes())];
    let old_log = paths.log.with_extension("log.old");
    for (name, path) in [
        ("project.json", &paths.project),
        ("settings.json", &paths.settings),
        ("soundbimba.log", &paths.log),
        ("soundbimba.log.old", &old_log),
    ] {
        if let Ok(data) = fs::read(path) {
            files.push((name.to_string(), data));
        }
    }
    fs::write(target, zip::build(&files, &winaudio::system::local_time()))
}
