// SPDX-License-Identifier: AGPL-3.0-or-later
//! Файлы приложения в %APPDATA%\SoundBimba: проект (автосохранение), настройки, журнал.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use graph::Project;
use serde::{Deserialize, Serialize};

use crate::applog;

pub struct Paths {
    pub dir: PathBuf,
    pub project: PathBuf,
    pub settings: PathBuf,
    pub log: PathBuf,
}

impl Paths {
    pub fn new() -> Paths {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        Paths::in_dir(base.join("SoundBimba"))
    }

    pub fn in_dir(dir: PathBuf) -> Paths {
        Paths {
            project: dir.join("project.json"),
            settings: dir.join("settings.json"),
            log: dir.join("logs").join("soundbimba.log"),
            dir,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// None — язык системы.
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Горячие клавиши (только внутри окна): действие → сочетание, «» — отключено.
    #[serde(default)]
    pub hotkeys: BTreeMap<String, String>,
    /// Звук включён (запоминается между запусками; при первом запуске — выключен).
    #[serde(default)]
    pub audio_enabled: bool,
}

fn default_theme() -> String {
    "bimba".into()
}

impl Default for Settings {
    fn default() -> Self {
        Settings { language: None, theme: default_theme(), hotkeys: BTreeMap::new(), audio_enabled: false }
    }
}

/// Записывает файл целиком или не трогает старый: сначала во временный файл, потом замена.
pub fn write_atomic(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}

pub struct Loaded {
    pub project: Project,
    /// Имя копии повреждённого файла, если пришлось начать заново.
    pub recovered_from: Option<String>,
}

pub fn load_project(paths: &Paths) -> Loaded {
    let fresh = || Loaded { project: graph::default_project(), recovered_from: None };
    let text = match fs::read_to_string(&paths.project) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            applog::info("no project file yet, starting with the default project");
            return fresh();
        }
        Err(e) => {
            applog::error(&format!("cannot read project file: {e}"));
            return fresh();
        }
    };
    match graph::from_json(&text) {
        Ok(project) => Loaded { project, recovered_from: None },
        Err(e) => {
            // Повреждённый файл не удаляем: откладываем копию рядом и начинаем с нового проекта.
            let name = format!("project.broken-{}.json", winaudio::system::local_time().replace([' ', ':'], "-"));
            let backup = paths.dir.join(&name);
            applog::error(&format!("project file is invalid ({e}); saved a copy as {name}"));
            match fs::rename(&paths.project, &backup) {
                Ok(()) => Loaded { project: graph::default_project(), recovered_from: Some(name) },
                Err(err) => {
                    applog::error(&format!("cannot back up the invalid project: {err}"));
                    fresh()
                }
            }
        }
    }
}

pub fn load_settings(paths: &Paths) -> Settings {
    fs::read_to_string(&paths.settings)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("soundbimba-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_project_gives_default() {
        let paths = Paths::in_dir(temp_dir("missing"));
        let l = load_project(&paths);
        assert_eq!(l.project, graph::default_project());
        assert!(l.recovered_from.is_none());
        let _ = fs::remove_dir_all(&paths.dir);
    }

    #[test]
    fn broken_project_is_kept_as_backup() {
        let paths = Paths::in_dir(temp_dir("broken"));
        fs::write(&paths.project, "{ broken").unwrap();
        let l = load_project(&paths);
        let name = l.recovered_from.expect("backup name");
        assert_eq!(fs::read_to_string(paths.dir.join(name)).unwrap(), "{ broken");
        assert!(!paths.project.exists());
        let _ = fs::remove_dir_all(&paths.dir);
    }

    #[test]
    fn saved_project_loads_back() {
        let paths = Paths::in_dir(temp_dir("roundtrip"));
        let mut p = graph::default_project();
        p.nodes[0].pos = [12.5, -3.0];
        write_atomic(&paths.project, &graph::to_json(&p)).unwrap();
        assert_eq!(load_project(&paths).project, p);
        let _ = fs::remove_dir_all(&paths.dir);
    }

    #[test]
    fn settings_have_defaults() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Settings::default());
        let s: Settings = serde_json::from_str(r#"{"language":"uk","theme":"metro"}"#).unwrap();
        assert_eq!(s.language.as_deref(), Some("uk"));
        let s: Settings = serde_json::from_str(r#"{"theme":"bimba","hotkeys":{"deleteSelected":"Backspace","arrange":""}}"#).unwrap();
        assert_eq!(s.hotkeys["deleteSelected"], "Backspace");
        assert_eq!(s.hotkeys["arrange"], "");
    }
}
