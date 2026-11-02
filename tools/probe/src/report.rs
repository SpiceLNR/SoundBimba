// SPDX-License-Identifier: AGPL-3.0-or-later
//! Отчёт: всё, что печатается в консоль, копится и дописывается в probe-report.txt.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

#[derive(Default)]
pub struct Report {
    buf: String,
}

impl Report {
    pub fn line(&mut self, s: impl AsRef<str>) {
        let s = s.as_ref();
        println!("{s}");
        self.buf.push_str(s);
        self.buf.push_str("\r\n");
    }

    pub fn section(&mut self, title: &str) {
        self.line("");
        self.line(format!("== {title} =="));
    }

    /// Печатает ошибку шага понятным текстом с кодом.
    pub fn error(&mut self, step: &str, e: &windows_core::Error) {
        self.line(format!(
            "  ОШИБКА на шаге «{step}»: {} (код 0x{:08X})",
            e.message().trim(),
            e.code().0 as u32
        ));
    }

    /// Дописывает накопленный текст в файл (создаёт его с BOM, чтобы Блокнот понял UTF-8).
    pub fn append_to(&self, path: &Path) -> std::io::Result<()> {
        let is_new = !path.exists();
        let mut f = OpenOptions::new().create(true).append(true).open(path)?;
        if is_new {
            f.write_all("\u{FEFF}".as_bytes())?;
        }
        f.write_all(self.buf.as_bytes())?;
        f.write_all(b"\r\n")
    }
}

/// `rep!(r, "формат {}", x)` — короткая запись строки отчёта.
#[macro_export]
macro_rules! rep {
    ($r:expr, $($arg:tt)*) => { $r.line(format!($($arg)*)) };
}
