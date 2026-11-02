// SPDX-License-Identifier: AGPL-3.0-or-later
//! Сведения о Windows и WebView2 из реестра, местное время для журналов.

use std::ffi::c_void;

use windows::core::PCWSTR;
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};
use windows::Win32::System::SystemInformation::GetLocalTime;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn reg_string(root: HKEY, key: &str, value: &str) -> Option<String> {
    let (k, v) = (wide(key), wide(value));
    let (k, v) = (PCWSTR(k.as_ptr()), PCWSTR(v.as_ptr()));
    let mut size = 0u32;
    unsafe {
        RegGetValueW(root, k, v, RRF_RT_REG_SZ, None, None, Some(&mut size)).ok().ok()?;
        let mut buf = vec![0u16; size as usize / 2 + 1];
        RegGetValueW(root, k, v, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut c_void), Some(&mut size))
            .ok()
            .ok()?;
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }
}

pub fn reg_dword(root: HKEY, key: &str, value: &str) -> Option<u32> {
    let (k, v) = (wide(key), wide(value));
    let (mut data, mut size) = (0u32, 4u32);
    unsafe {
        RegGetValueW(
            root,
            PCWSTR(k.as_ptr()),
            PCWSTR(v.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }
    Some(data)
}

pub struct WindowsInfo {
    pub product: String,
    pub edition: String,
    pub display_version: String,
    pub build: String,
    pub ubr: Option<u32>,
}

impl WindowsInfo {
    /// Windows 11 в реестре часто называет себя «Windows 10»; надёжнее номер сборки.
    pub fn is_windows_11(&self) -> bool {
        self.build.parse::<u32>().is_ok_and(|b| b >= 22000)
    }

    pub fn summary(&self) -> String {
        let family = if self.is_windows_11() { "Windows 11" } else { "Windows 10 or older" };
        let ubr = self.ubr.map(|u| format!(".{u}")).unwrap_or_default();
        format!(
            "{family} | {} | {} | {} | build {}{ubr}",
            self.product, self.edition, self.display_version, self.build
        )
    }
}

pub fn windows_info() -> WindowsInfo {
    const CV: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let get = |v| reg_string(HKEY_LOCAL_MACHINE, CV, v).unwrap_or_else(|| "?".into());
    WindowsInfo {
        product: get("ProductName"),
        edition: get("EditionID"),
        display_version: get("DisplayVersion"),
        build: get("CurrentBuild"),
        ubr: reg_dword(HKEY_LOCAL_MACHINE, CV, "UBR"),
    }
}

/// Версия WebView2 Runtime из реестра (обе ветки HKLM и HKCU) или None.
pub fn webview2_version() -> Option<String> {
    const ID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    [
        (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{ID}")),
        (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{ID}")),
        (HKEY_CURRENT_USER, format!(r"Software\Microsoft\EdgeUpdate\Clients\{ID}")),
    ]
    .iter()
    .filter_map(|(root, key)| reg_string(*root, key, "pv"))
    .find(|v| !v.is_empty() && v != "0.0.0.0")
}

/// Местное время в виде «2026-11-02 14:05:09».
pub fn local_time() -> String {
    let t = unsafe { GetLocalTime() };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn local_time_has_fixed_shape() {
        let t = super::local_time();
        assert_eq!(t.len(), 19);
        assert_eq!(&t[4..5], "-");
        assert_eq!(&t[10..11], " ");
    }

    #[test]
    fn windows_build_is_readable() {
        // Тест на реальной системе: номер сборки есть в реестре любой Windows 10/11.
        assert!(super::windows_info().build.parse::<u32>().is_ok());
    }
}
