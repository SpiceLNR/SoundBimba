// SPDX-License-Identifier: AGPL-3.0-or-later
//! SoundBimba: окно Tauri и команды для интерфейса (загрузка и автосохранение проекта, настройки,
//! экспорт и импорт, диагностика). Звука на этапе 1 ещё нет.

// В выпускной сборке не показывать окно консоли.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod applog;
mod audio;
mod diagnostics;
mod storage;
mod zip;

use std::fs;
use std::sync::Mutex;

use graph::Project;
use serde::Serialize;
use audio::Audio;
use storage::{Paths, Settings};
use tauri::{Emitter, Manager, RunEvent, State};

struct AppState {
    paths: Paths,
    /// Запись файлов по одной, чтобы автосохранения не наложились.
    write_lock: Mutex<()>,
}

/// Ошибка для интерфейса: `code` — ключ локализации, `detail` — подробность для журнала.
#[derive(Serialize)]
struct CmdError {
    code: String,
    detail: String,
}

impl CmdError {
    fn new(code: &str, detail: impl ToString) -> Self {
        let e = CmdError { code: code.into(), detail: detail.to_string() };
        applog::error(&format!("{}: {}", e.code, e.detail));
        e
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LoadState {
    project: Project,
    settings: Settings,
    recovered_from: Option<String>,
    version: &'static str,
}

#[tauri::command]
fn load_state(state: State<AppState>, audio: State<Audio>) -> LoadState {
    let loaded = storage::load_project(&state.paths);
    let settings = storage::load_settings(&state.paths);
    audio.set_project(loaded.project.clone());
    audio.set_enabled(settings.audio_enabled);
    LoadState { project: loaded.project, settings, recovered_from: loaded.recovered_from, version: env!("CARGO_PKG_VERSION") }
}

/// Проект и настройки без побочных действий — для кнопочной панели (второе окно).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UiState {
    project: Project,
    settings: Settings,
}

#[tauri::command]
fn get_ui_state(state: State<AppState>) -> UiState {
    UiState { project: storage::load_project(&state.paths).project, settings: storage::load_settings(&state.paths) }
}

#[tauri::command]
fn save_project(app: tauri::AppHandle, state: State<AppState>, audio: State<Audio>, project: Project) -> Result<(), CmdError> {
    graph::validate(&project).map_err(|e| CmdError::new(e.code(), &e))?;
    audio.set_project(project.clone());
    // Кнопочная панель узнаёт о новых кнопках.
    let _ = app.emit("project-changed", &project);
    let _guard = state.write_lock.lock().unwrap();
    storage::write_atomic(&state.paths.project, &graph::to_json(&project)).map_err(|e| CmdError::new("io", e))
}

/// Нажатие кнопки-звука: играть, а если уже играет — стоп.
#[tauri::command]
fn press_button(audio: State<Audio>, node_id: String) {
    audio.press(&node_id);
}

/// Перемотка кнопки-звука: нажатие на кольцо (доля 0…1).
#[tauri::command]
fn seek_button(audio: State<Audio>, node_id: String, fraction: f32) {
    audio.seek(&node_id, fraction);
}

/// Включить или выключить звук (сама настройка сохраняется вместе с остальными).
#[tauri::command]
fn set_audio_enabled(audio: State<Audio>, enabled: bool) {
    audio.set_enabled(enabled);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceDto {
    id: String,
    name: String,
    render: bool,
    is_default: bool,
    cable: Option<&'static str>,
}

/// Звуковые устройства для выбора в боковой панели.
#[tauri::command]
fn list_audio_devices() -> Result<Vec<DeviceDto>, CmdError> {
    let devs = winaudio::devices::list().map_err(|e| CmdError::new("io", e.message()))?;
    Ok(devs
        .into_iter()
        .map(|d| DeviceDto { id: d.id, name: d.name, render: d.render, is_default: d.is_default, cable: d.cable.map(|c| c.label) })
        .collect())
}

#[derive(Serialize)]
struct AppDto {
    exe: String,
    active: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LatencyDto {
    /// Замер: SoundBimba → виртуальный микрофон, мс.
    output_ms: f64,
    /// Оценка входа (захват программы или микрофона → движок): запас буфера + порция устройства.
    input_estimate_ms: f64,
}

/// Замер задержки через виртуальный кабель (около 2 секунд).
#[tauri::command]
async fn measure_latency(app: tauri::AppHandle) -> Result<LatencyDto, CmdError> {
    let result = tauri::async_runtime::spawn_blocking(|| {
        winaudio::devices::ensure_com().map_err(|e| e.message())?;
        let devs = winaudio::devices::list().map_err(|e| e.message())?;
        let spk = winaudio::devices::primary_cable_render(&devs).ok_or("no-cable")?;
        let mic = winaudio::devices::primary_cable_capture(&devs).ok_or("no-cable")?;
        winaudio::latency::measure_cable(&spk.id, &mic.id, audio::RENDER_RESERVE_MS, audio::CAPTURE_RESERVE_MS)
    })
    .await
    .map_err(|e| CmdError::new("io", e))?;
    match result {
        Ok(ms) => {
            app.state::<Audio>().set_latency(ms);
            applog::info(&format!("latency measured: {ms:.1} ms"));
            Ok(LatencyDto { output_ms: ms, input_estimate_ms: audio::CAPTURE_RESERVE_MS + 10.0 })
        }
        Err(e) if e == "no-cable" => Err(CmdError::new("no-cable", e)),
        Err(e) => Err(CmdError::new("latency", e)),
    }
}

/// Программы, у которых сейчас есть звук, — для выбора в узле «Звук приложения».
#[tauri::command]
fn list_audio_apps() -> Result<Vec<AppDto>, CmdError> {
    let apps = winaudio::processes::audio_apps().map_err(|e| CmdError::new("io", e.message()))?;
    Ok(apps.into_iter().map(|a| AppDto { exe: a.exe, active: a.active }).collect())
}

#[tauri::command]
fn save_settings(app: tauri::AppHandle, state: State<AppState>, settings: Settings) -> Result<(), CmdError> {
    let _ = app.emit("settings-changed", &settings);
    let text = serde_json::to_string_pretty(&settings).map_err(|e| CmdError::new("io", e))?;
    let _guard = state.write_lock.lock().unwrap();
    storage::write_atomic(&state.paths.settings, &text).map_err(|e| CmdError::new("io", e))
}

#[tauri::command]
fn export_project(path: String, project: Project) -> Result<(), CmdError> {
    graph::validate(&project).map_err(|e| CmdError::new(e.code(), &e))?;
    fs::write(&path, graph::to_json(&project)).map_err(|e| CmdError::new("io", e))?;
    applog::info("project exported");
    Ok(())
}

#[tauri::command]
fn import_project(path: String) -> Result<Project, CmdError> {
    let text = fs::read_to_string(&path).map_err(|e| CmdError::new("io", e))?;
    let project = graph::from_json(&text).map_err(|e| CmdError::new(e.code(), &e))?;
    applog::info("project imported");
    Ok(project)
}

#[tauri::command]
fn save_diagnostics(state: State<AppState>, audio: State<Audio>, path: String, ui_info: String) -> Result<(), CmdError> {
    let snapshot = serde_json::to_string_pretty(&audio.snapshot()).unwrap_or_default();
    diagnostics::save(&state.paths, &path, &ui_info, &snapshot).map_err(|e| CmdError::new("io", e))?;
    applog::info("diagnostics saved");
    Ok(())
}

/// Ошибки и события интерфейса — в общий журнал.
#[tauri::command]
fn log_ui(level: String, message: String) {
    applog::write(if level == "error" { "UI-ERROR" } else { "UI" }, &message);
}

fn main() {
    let paths = Paths::new();
    applog::init(paths.log.clone());
    applog::info(&format!(
        "SoundBimba {} starting; {}",
        env!("CARGO_PKG_VERSION"),
        winaudio::system::windows_info().summary()
    ));
    std::panic::set_hook(Box::new(|info| applog::error(&format!("panic: {info}"))));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState { paths, write_lock: Mutex::new(()) })
        .setup(|app| {
            app.manage(Audio::start(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_state,
            save_project,
            save_settings,
            export_project,
            import_project,
            save_diagnostics,
            log_ui,
            set_audio_enabled,
            list_audio_devices,
            list_audio_apps,
            measure_latency,
            press_button,
            seek_button,
            get_ui_state
        ])
        .on_window_event(|window, event| {
            // Закрыли основное окно — закрываем и кнопочную панель, иначе программа не выйдет.
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                if let Some(panel) = window.app_handle().get_webview_window("buttons") {
                    let _ = panel.close();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building SoundBimba")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                // Закрыть устройства аккуратно, а не обрывом процесса.
                app.state::<Audio>().shutdown();
            }
        });
}
