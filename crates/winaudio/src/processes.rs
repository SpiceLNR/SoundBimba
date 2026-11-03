// SPDX-License-Identifier: AGPL-3.0-or-later
//! Процессы и аудиосессии: найти программу по имени exe, узнать, какие программы сейчас со звуком.

use windows::core::{Interface, Result};
use windows::Win32::Foundation::{CloseHandle, S_OK};
use windows::Win32::Media::Audio::{
    eRender, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
    MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

use crate::devices::ensure_com;

pub struct ProcessInfo {
    pub pid: u32,
    pub parent: u32,
    pub exe: String,
}

/// Все процессы системы (снимок).
pub fn list() -> Vec<ProcessInfo> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return out;
        };
        let mut e = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let end = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            out.push(ProcessInfo {
                pid: e.th32ProcessID,
                parent: e.th32ParentProcessID,
                exe: String::from_utf16_lossy(&e.szExeFile[..end]),
            });
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// Главный процесс программы по имени exe — тот, чей родитель — другая программа.
/// У браузеров десятки процессов; захват «с деревом» от главного берёт звук всех.
pub fn find_root(procs: &[ProcessInfo], exe: &str) -> Option<u32> {
    let same: Vec<&ProcessInfo> = procs.iter().filter(|p| p.exe.eq_ignore_ascii_case(exe)).collect();
    let is_same = |pid: u32| same.iter().any(|p| p.pid == pid);
    same.iter().find(|p| !is_same(p.parent)).or(same.first()).map(|p| p.pid)
}

pub struct AudioApp {
    pub exe: String,
    pub pid: u32,
    /// Играет ли звук прямо сейчас.
    pub active: bool,
}

/// Все процессы с аудиосессией на устройствах вывода: (имя exe, PID). У одной программы их
/// может быть несколько (браузер играет звук из отдельного процесса). Без системных звуков и нас.
pub fn audio_sessions() -> Result<Vec<(String, u32)>> {
    ensure_com()?;
    let procs = list();
    let me = std::process::id();
    let mut out: Vec<(String, u32)> = Vec::new();
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let devs = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        for i in 0..devs.GetCount()? {
            let Ok(mgr) = devs.Item(i)?.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
            let sessions = mgr.GetSessionEnumerator()?;
            for s in 0..sessions.GetCount()? {
                let ctl = sessions.GetSession(s)?;
                let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
                let pid = ctl2.GetProcessId().unwrap_or(0);
                if pid == 0 || pid == me || ctl2.IsSystemSoundsSession() == S_OK {
                    continue;
                }
                let Some(exe) = procs.iter().find(|p| p.pid == pid).map(|p| p.exe.clone()) else { continue };
                if !out.iter().any(|(_, p)| *p == pid) {
                    out.push((exe, pid));
                }
            }
        }
    }
    Ok(out)
}

/// Программы с аудиосессией на любом устройстве вывода (без системных звуков и без нас самих).
pub fn audio_apps() -> Result<Vec<AudioApp>> {
    ensure_com()?;
    let procs = list();
    let me = std::process::id();
    let mut out: Vec<AudioApp> = Vec::new();
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let devs = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        for i in 0..devs.GetCount()? {
            let Ok(mgr) = devs.Item(i)?.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
            let sessions = mgr.GetSessionEnumerator()?;
            for s in 0..sessions.GetCount()? {
                let ctl = sessions.GetSession(s)?;
                let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
                let pid = ctl2.GetProcessId().unwrap_or(0);
                if pid == 0 || pid == me || ctl2.IsSystemSoundsSession() == S_OK {
                    continue;
                }
                let Some(exe) = procs.iter().find(|p| p.pid == pid).map(|p| p.exe.clone()) else { continue };
                let active = ctl.GetState().map(|st| st == AudioSessionStateActive).unwrap_or(false);
                match out.iter_mut().find(|a| a.exe.eq_ignore_ascii_case(&exe)) {
                    Some(a) => a.active |= active,
                    None => out.push(AudioApp { exe, pid, active }),
                }
            }
        }
    }
    out.sort_by(|a, b| b.active.cmp(&a.active).then_with(|| a.exe.to_lowercase().cmp(&b.exe.to_lowercase())));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, parent: u32, exe: &str) -> ProcessInfo {
        ProcessInfo { pid, parent, exe: exe.into() }
    }

    #[test]
    fn root_process_is_the_one_started_by_another_program() {
        let procs = vec![p(10, 1, "explorer.exe"), p(20, 10, "chrome.exe"), p(21, 20, "chrome.exe"), p(22, 20, "Chrome.EXE")];
        assert_eq!(find_root(&procs, "chrome.exe"), Some(20));
        assert_eq!(find_root(&procs, "discord.exe"), None);
    }

    #[test]
    fn this_process_is_listed() {
        // Тест на реальной системе: снимок процессов содержит и нас.
        let me = std::process::id();
        assert!(list().iter().any(|p| p.pid == me));
    }
}
