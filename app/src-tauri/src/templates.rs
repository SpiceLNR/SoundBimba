// SPDX-License-Identifier: AGPL-3.0-or-later
//! Свои шаблоны: файлы `.bmb` (JSON) в папке `templates` рядом с проектом.
//!
//! Внутри файла — метка формата, чтобы чужой файл с тем же расширением не приняли за шаблон:
//! `{"format": "soundbimba-template", "version": 1, "name", "description", "project"}`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use graph::Project;
use serde::{Deserialize, Serialize};

use crate::storage::write_atomic;

pub const EXTENSION: &str = "bmb";
const FORMAT: &str = "soundbimba-template";
const VERSION: u32 = 1;
/// Шаблон — небольшой текст; больше — явно не наш файл.
const MAX_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_NAME: usize = 80;
pub const MAX_DESCRIPTION: usize = 400;

#[derive(Serialize, Deserialize)]
struct FileData {
    format: String,
    version: u32,
    name: String,
    #[serde(default)]
    description: String,
    project: serde_json::Value,
}

/// Шаблон из папки пользователя. `id` — имя файла без `.bmb`.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct UserTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub project: Project,
}

/// Почему шаблон не принят. `code()` — ключ причины для интерфейса (`reason.<code>`).
#[derive(Debug)]
pub enum TemplateError {
    Io(io::Error),
    TooBig,
    NotTemplate,
    NewerVersion,
    EmptyName,
    Project(&'static str, String),
}

impl TemplateError {
    pub fn code(&self) -> &'static str {
        match self {
            TemplateError::Io(_) => "io",
            TemplateError::TooBig | TemplateError::NotTemplate => "not-template",
            TemplateError::NewerVersion => "newer-template",
            TemplateError::EmptyName => "empty-name",
            TemplateError::Project(code, _) => code,
        }
    }
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::Io(e) => write!(f, "{e}"),
            TemplateError::TooBig => write!(f, "file is larger than {MAX_BYTES} bytes"),
            TemplateError::NotTemplate => write!(f, "not a SoundBimba template"),
            TemplateError::NewerVersion => write!(f, "template made by a newer SoundBimba"),
            TemplateError::EmptyName => write!(f, "template name is empty"),
            TemplateError::Project(_, e) => write!(f, "template project is invalid: {e}"),
        }
    }
}

impl From<io::Error> for TemplateError {
    fn from(e: io::Error) -> Self {
        TemplateError::Io(e)
    }
}

fn clip(s: &str, max: usize) -> String {
    s.trim().chars().take(max).collect()
}

/// Разбирает текст файла шаблона. Проект проверяется теми же правилами, что и обычный.
pub fn parse(text: &str) -> Result<(String, String, Project), TemplateError> {
    let data: FileData = serde_json::from_str(text).map_err(|_| TemplateError::NotTemplate)?;
    if data.format != FORMAT {
        return Err(TemplateError::NotTemplate);
    }
    if data.version > VERSION {
        return Err(TemplateError::NewerVersion);
    }
    let name = clip(&data.name, MAX_NAME);
    if name.is_empty() {
        return Err(TemplateError::EmptyName);
    }
    let project = graph::from_json(&data.project.to_string()).map_err(|e| TemplateError::Project(e.code(), e.to_string()))?;
    Ok((name, clip(&data.description, MAX_DESCRIPTION), project))
}

pub fn to_text(name: &str, description: &str, project: &Project) -> String {
    let data = FileData {
        format: FORMAT.into(),
        version: VERSION,
        name: name.into(),
        description: description.into(),
        project: serde_json::from_str(&graph::to_json(project)).unwrap_or_default(),
    };
    serde_json::to_string_pretty(&data).unwrap_or_default()
}

fn read_limited(path: &Path) -> Result<String, TemplateError> {
    if fs::metadata(path)?.len() > MAX_BYTES {
        return Err(TemplateError::TooBig);
    }
    Ok(fs::read_to_string(path)?)
}

/// Свои шаблоны по названию. Испорченные файлы пропускаются (и попадают в журнал).
pub fn list(dir: &Path) -> Vec<UserTemplate> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<UserTemplate> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(EXTENSION)))
        .filter_map(|p| {
            let id = p.file_stem()?.to_string_lossy().into_owned();
            match read_limited(&p).and_then(|t| parse(&t)) {
                Ok((name, description, project)) => Some(UserTemplate { id, name, description, project }),
                Err(e) => {
                    crate::applog::error(&format!("template {}: {e}", p.display()));
                    None
                }
            }
        })
        .collect();
    out.sort_by_key(|t| t.name.to_lowercase());
    out
}

/// Имя файла из названия: без запрещённых в Windows символов, не длиннее 60 знаков.
fn file_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .take(60)
        .collect();
    let s = s.trim().trim_end_matches('.').trim().to_string();
    // Зарезервированные имена устройств Windows.
    let upper = s.to_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str())
        || (upper.len() == 4 && (upper.starts_with("COM") || upper.starts_with("LPT")) && upper.as_bytes()[3].is_ascii_digit());
    if s.is_empty() || reserved {
        format!("template {s}").trim().to_string()
    } else {
        s
    }
}

fn free_path(dir: &Path, stem: &str) -> (String, PathBuf) {
    let mut id = stem.to_string();
    let mut n = 2;
    while dir.join(format!("{id}.{EXTENSION}")).exists() {
        id = format!("{stem} ({n})");
        n += 1;
    }
    let path = dir.join(format!("{id}.{EXTENSION}"));
    (id, path)
}

/// Сохраняет новый шаблон (существующие не перезаписываются).
pub fn save(dir: &Path, name: &str, description: &str, project: &Project) -> Result<UserTemplate, TemplateError> {
    let name = clip(name, MAX_NAME);
    if name.is_empty() {
        return Err(TemplateError::EmptyName);
    }
    let description = clip(description, MAX_DESCRIPTION);
    graph::validate(project).map_err(|e| TemplateError::Project(e.code(), e.to_string()))?;
    fs::create_dir_all(dir)?;
    let (id, path) = free_path(dir, &file_stem(&name));
    write_atomic(&path, &to_text(&name, &description, project))?;
    Ok(UserTemplate { id, name, description, project: project.clone() })
}

/// Путь к своему шаблону по id — только внутри папки шаблонов.
fn own_path(dir: &Path, id: &str) -> Option<PathBuf> {
    let plain = !id.is_empty() && !id.contains(['/', '\\', ':']) && id != "." && id != "..";
    plain.then(|| dir.join(format!("{id}.{EXTENSION}")))
}

pub fn delete(dir: &Path, id: &str) -> Result<(), TemplateError> {
    let path = own_path(dir, id).ok_or(TemplateError::NotTemplate)?;
    Ok(fs::remove_file(path)?)
}

/// Добавляет шаблон из файла (например, присланного другом) в свои.
pub fn import(dir: &Path, src: &Path) -> Result<UserTemplate, TemplateError> {
    let (name, description, project) = parse(&read_limited(src)?)?;
    save(dir, &name, &description, &project)
}

pub fn export(dir: &Path, id: &str, dst: &Path) -> Result<(), TemplateError> {
    let path = own_path(dir, id).ok_or(TemplateError::NotTemplate)?;
    let (name, description, project) = parse(&read_limited(&path)?)?;
    Ok(fs::write(dst, to_text(&name, &description, &project))?)
}

/// Файлы `.bmb` среди аргументов запуска (двойной щелчок по файлу в Проводнике).
pub fn opened_files<I: IntoIterator<Item = String>>(args: I) -> Vec<PathBuf> {
    args.into_iter()
        .skip(1)
        .map(PathBuf::from)
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(EXTENSION)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-templates-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn save_list_export_import_delete() {
        let dir = temp_dir("roundtrip");
        let p = graph::default_project();
        let a = save(&dir, "  Мой стрим  ", "Музыка тише голоса", &p).unwrap();
        assert_eq!(a.name, "Мой стрим");
        let b = save(&dir, "Мой стрим", "", &p).unwrap();
        assert_ne!(a.id, b.id, "одинаковые названия — разные файлы");
        assert_eq!(list(&dir).len(), 2);

        let out = dir.join("export.bmb.out");
        export(&dir, &a.id, &out).unwrap();
        let other = temp_dir("other");
        let imported = import(&other, &out).unwrap();
        assert_eq!((imported.name.as_str(), imported.description.as_str()), ("Мой стрим", "Музыка тише голоса"));
        assert_eq!(imported.project, p);

        delete(&dir, &a.id).unwrap();
        assert_eq!(list(&dir).len(), 1);
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&other);
    }

    #[test]
    fn foreign_files_are_rejected() {
        assert_eq!(parse("not json").unwrap_err().code(), "not-template");
        assert_eq!(parse(r#"{"format":"other","version":1,"name":"x","project":{}}"#).unwrap_err().code(), "not-template");
        let newer = format!(r#"{{"format":"{FORMAT}","version":99,"name":"x","project":{{}}}}"#);
        assert_eq!(parse(&newer).unwrap_err().code(), "newer-template");
        let cycle = format!(
            r#"{{"format":"{FORMAT}","version":1,"name":"x","project":{{"schema":1,"nodes":[
            {{"id":"a","type":"volume","pos":[0,0],"params":{{}}}},{{"id":"b","type":"volume","pos":[0,0],"params":{{}}}}],
            "edges":[{{"from":"a","to":"b"}},{{"from":"b","to":"a"}}]}}}}"#
        );
        assert_eq!(parse(&cycle).unwrap_err().code(), "cycle");
        let unnamed = format!(r#"{{"format":"{FORMAT}","version":1,"name":"  ","project":{{"schema":1,"nodes":[],"edges":[]}}}}"#);
        assert_eq!(parse(&unnamed).unwrap_err().code(), "empty-name");
    }

    #[test]
    fn ids_cannot_leave_the_folder() {
        let dir = PathBuf::from("C:/x");
        assert!(own_path(&dir, "..").is_none());
        assert!(own_path(&dir, "../project").is_none());
        assert!(own_path(&dir, "C:\\Windows\\a").is_none());
        assert!(own_path(&dir, "Мой стрим (2)").is_some());
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_stem("a/b:c*?"), "a_b_c__");
        assert_eq!(file_stem("CON"), "template CON");
        assert_eq!(file_stem("..."), "template");
        assert_eq!(file_stem("Звук игры."), "Звук игры");
    }

    #[test]
    fn only_bmb_arguments_count_as_opened() {
        let args = ["soundbimba.exe", "C:\\a\\Стрим.bmb", "--flag", "C:\\b.BMB", "x.json"].map(String::from);
        assert_eq!(opened_files(args), vec![PathBuf::from("C:\\a\\Стрим.bmb"), PathBuf::from("C:\\b.BMB")]);
    }
}
