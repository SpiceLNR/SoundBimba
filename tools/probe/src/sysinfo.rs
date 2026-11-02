// SPDX-License-Identifier: AGPL-3.0-or-later
//! Шаги 1–2: версия Windows и наличие WebView2. Только чтение реестра.

use std::ffi::c_void;

use windows::core::PCWSTR;
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};
use windows::Win32::System::SystemInformation::{
    GetNativeSystemInfo, PROCESSOR_ARCHITECTURE_AMD64, PROCESSOR_ARCHITECTURE_ARM64, SYSTEM_INFO,
};

use crate::report::Report;
use crate::rep;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn reg_string(root: HKEY, key: &str, value: &str) -> Option<String> {
    let (k, v) = (wide(key), wide(value));
    let (k, v) = (PCWSTR(k.as_ptr()), PCWSTR(v.as_ptr()));
    let mut size = 0u32;
    unsafe {
        if RegGetValueW(root, k, v, RRF_RT_REG_SZ, None, None, Some(&mut size)).is_err() {
            return None;
        }
        let mut buf = vec![0u16; size as usize / 2 + 1];
        if RegGetValueW(
            root,
            k,
            v,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
        .is_err()
        {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }
}

fn reg_dword(root: HKEY, key: &str, value: &str) -> Option<u32> {
    let (k, v) = (wide(key), wide(value));
    let mut data = 0u32;
    let mut size = 4u32;
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

pub fn system(r: &mut Report) {
    r.section("1. Система");
    const CV: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let get = |v| reg_string(HKEY_LOCAL_MACHINE, CV, v).unwrap_or_else(|| "?".into());
    let build = get("CurrentBuild");
    let ubr = reg_dword(HKEY_LOCAL_MACHINE, CV, "UBR")
        .map(|u| u.to_string())
        .unwrap_or_else(|| "?".into());
    let family = match build.parse::<u32>() {
        Ok(b) if b >= 22000 => "Windows 11",
        Ok(_) => "Windows 10 или старше",
        Err(_) => "не удалось определить",
    };
    rep!(r, "  Семейство:        {family} (по номеру сборки)");
    rep!(r, "  Название в реестре: {} (Windows 11 часто пишет здесь «Windows 10» — это нормально)", get("ProductName"));
    rep!(r, "  Редакция:         {}", get("EditionID"));
    rep!(r, "  Версия:           {}", get("DisplayVersion"));
    rep!(r, "  Сборка:           {build}.{ubr}");

    let mut si = SYSTEM_INFO::default();
    unsafe { GetNativeSystemInfo(&mut si) };
    let arch = unsafe { si.Anonymous.Anonymous.wProcessorArchitecture };
    let arch = match arch {
        PROCESSOR_ARCHITECTURE_AMD64 => "x64 (64-бит)".to_string(),
        PROCESSOR_ARCHITECTURE_ARM64 => "ARM64".to_string(),
        other => format!("другая (код {})", other.0),
    };
    rep!(r, "  Разрядность:      {arch}");
}

pub fn webview2(r: &mut Report) {
    r.section("2. WebView2 (движок окна будущей программы)");
    const ID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let places = [
        (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{ID}"), "HKLM (32-бит ветка)"),
        (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{ID}"), "HKLM"),
        (HKEY_CURRENT_USER, format!(r"Software\Microsoft\EdgeUpdate\Clients\{ID}"), "HKCU (для пользователя)"),
    ];
    let mut found = None;
    for (root, key, label) in &places {
        match reg_string(*root, key, "pv") {
            Some(v) if !v.is_empty() && v != "0.0.0.0" => {
                rep!(r, "  {label}: версия {v}");
                found.get_or_insert(v);
            }
            _ => rep!(r, "  {label}: нет"),
        }
    }
    match found {
        Some(v) => rep!(r, "  ИТОГ: WebView2 установлен, версия {v}"),
        None => rep!(r, "  ИТОГ: WebView2 НЕ найден (установщик программы должен будет его поставить)"),
    }
}
