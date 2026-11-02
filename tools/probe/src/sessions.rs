// SPDX-License-Identifier: AGPL-3.0-or-later
//! Шаг 5: какие программы сейчас имеют звуковую сессию. Плюс поиск процесса по имени exe.

use windows::core::{Interface, Result, PWSTR};
use windows::Win32::Foundation::{CloseHandle, S_OK};
use windows::Win32::Media::Audio::{
    AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, ISimpleAudioVolume,
};
use windows::Win32::System::Com::CLSCTX_ALL;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::devices::Device;
use crate::report::Report;
use crate::rep;

pub struct Session {
    pub device: String,
    pub pid: u32,
    pub exe: String,
    pub active: bool,
    pub volume: Option<f32>,
    pub mute: bool,
    pub system: bool,
}

/// Имя exe по PID, например «chrome.exe». None, если процесса нет.
pub fn process_name(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    // Путь к exe закрыт у процессов с правами администратора — тогда берём имя из списка процессов.
    image_name(pid).or_else(|| processes().into_iter().find(|p| p.0 == pid).map(|p| p.2))
}

fn image_name(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        ok.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        Some(path.rsplit('\\').next().unwrap_or(&path).to_string())
    }
}

/// Все процессы: (pid, pid родителя, имя exe).
fn processes() -> Vec<(u32, u32, String)> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return out;
        };
        let mut e = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let end = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            out.push((e.th32ProcessID, e.th32ParentProcessID, String::from_utf16_lossy(&e.szExeFile[..end])));
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// Находит главный процесс программы по имени exe (тот, чей родитель — другая программа).
/// У браузеров десятки процессов; захват «с деревом» от главного берёт звук всех.
pub fn find_pid_by_exe(exe: &str) -> Option<u32> {
    let procs = processes();
    let same: Vec<_> = procs.iter().filter(|p| p.2.eq_ignore_ascii_case(exe)).collect();
    let is_same = |pid: u32| same.iter().any(|p| p.0 == pid);
    same.iter()
        .find(|p| !is_same(p.1))
        .or(same.first())
        .map(|p| p.0)
}

pub fn list(devs: &[Device]) -> (Vec<Session>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for d in devs.iter().filter(|d| d.render) {
        if let Err(e) = list_device(d, &mut out) {
            errors.push(format!(
                "«{}»: {} (код 0x{:08X})",
                d.name,
                e.message().trim(),
                e.code().0 as u32
            ));
        }
    }
    (out, errors)
}

fn list_device(d: &Device, out: &mut Vec<Session>) -> Result<()> {
    unsafe {
        let mgr = d.dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)?;
        let en = mgr.GetSessionEnumerator()?;
        for i in 0..en.GetCount()? {
            let ctl = en.GetSession(i)?;
            let ctl2: IAudioSessionControl2 = ctl.cast()?;
            let pid = ctl2.GetProcessId().unwrap_or(0);
            let system = ctl2.IsSystemSoundsSession() == S_OK;
            let active = ctl.GetState().map(|s| s == AudioSessionStateActive).unwrap_or(false);
            let vol: Option<ISimpleAudioVolume> = ctl.cast().ok();
            out.push(Session {
                device: d.name.clone(),
                pid,
                exe: if system {
                    "Системные звуки".into()
                } else {
                    process_name(pid).unwrap_or_else(|| "?".into())
                },
                active,
                volume: vol.as_ref().and_then(|v| v.GetMasterVolume().ok()),
                mute: vol.as_ref().and_then(|v| v.GetMute().ok()).is_some_and(|m| m.as_bool()),
                system,
            });
        }
    }
    Ok(())
}

/// Программы, звук которых можно записать (без системных звуков, по одному на PID).
pub fn capturable(sessions: &[Session]) -> Vec<&Session> {
    let mut v: Vec<&Session> = Vec::new();
    for s in sessions.iter().filter(|s| !s.system && s.pid != 0) {
        if !v.iter().any(|x| x.pid == s.pid) {
            v.push(s);
        }
    }
    v
}

pub fn report(r: &mut Report, sessions: &[Session], errors: &[String]) {
    r.section("5. Программы со звуком (аудиосессии)");
    if sessions.is_empty() {
        rep!(r, "  Сессий нет. Запустите программу со звуком (браузер, плеер) и повторите.");
    }
    for s in sessions {
        let state = if s.active { "играет сейчас" } else { "молчит" };
        let vol = s.volume.map(|v| format!("{:.0} %", v * 100.0)).unwrap_or_else(|| "?".into());
        let mute = if s.mute { ", звук выключен" } else { "" };
        rep!(r, "  • {} (PID {}) — {state}, громкость {vol}{mute}; устройство «{}»", s.exe, s.pid, s.device);
    }
    for e in errors {
        rep!(r, "  не удалось прочитать сессии устройства {e}");
    }
}
