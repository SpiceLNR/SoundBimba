// SPDX-License-Identifier: AGPL-3.0-or-later
//! Значок SoundBimba в области уведомлений (трее, возле часов). Щелчок — показать окно; меню по
//! правому щелчку — «Показать SoundBimba» и «Выйти». Крестик окна прячет его в трей (настройка
//! `closeToTray`, по умолчанию включена), звук при этом работает дальше.
//!
//! Подписи меню приходят из интерфейса (там локализация): значок появляется, когда интерфейс
//! загрузился и прислал их. Пока значка нет, крестик закрывает программу как обычно — иначе
//! спрятанное окно было бы нечем вернуть.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Deserialize;
use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::applog;

pub struct Tray {
    close_to_tray: AtomicBool,
    parts: Mutex<Option<Parts>>,
}

struct Parts {
    show: MenuItem<Wry>,
    quit: MenuItem<Wry>,
    icon: TrayIcon<Wry>,
}

/// Подписи на языке интерфейса.
#[derive(Deserialize)]
pub struct TrayTexts {
    show: String,
    quit: String,
    tooltip: String,
}

impl Tray {
    pub fn new(close_to_tray: bool) -> Self {
        Tray { close_to_tray: AtomicBool::new(close_to_tray), parts: Mutex::new(None) }
    }

    pub fn set_close_to_tray(&self, on: bool) {
        self.close_to_tray.store(on, Ordering::Relaxed);
    }

    /// Крестик прячет окно: настройка включена и значок уже есть.
    pub fn hides_on_close(&self) -> bool {
        self.close_to_tray.load(Ordering::Relaxed) && self.parts.lock().unwrap().is_some()
    }

    /// Первый вызов создаёт значок, следующие (сменили язык) только меняют подписи.
    pub fn set_texts(&self, app: &AppHandle, t: TrayTexts) -> tauri::Result<()> {
        let mut parts = self.parts.lock().unwrap();
        if let Some(p) = parts.as_ref() {
            p.show.set_text(&t.show)?;
            p.quit.set_text(&t.quit)?;
            return p.icon.set_tooltip(Some(&t.tooltip));
        }
        let show = MenuItem::with_id(app, "show", &t.show, true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "quit", &t.quit, true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&show, &quit])?;
        let mut builder = TrayIconBuilder::with_id("main")
            .tooltip(&t.tooltip)
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(on_menu)
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                    show_main(tray.app_handle());
                }
            });
        if let Some(image) = app.default_window_icon() {
            builder = builder.icon(image.clone());
        }
        let icon = builder.build(app)?;
        applog::info("tray icon added");
        *parts = Some(Parts { show, quit, icon });
        Ok(())
    }
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        "show" => show_main(app),
        "quit" => {
            applog::info("quit from the tray");
            app.exit(0);
        }
        _ => {}
    }
}

/// Вернуть основное окно: показать, развернуть из панели задач, дать фокус.
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
