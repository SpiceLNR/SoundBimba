// SPDX-License-Identifier: AGPL-3.0-or-later
//! Куда Windows направляет звук отдельной программы — то же, что «Параметры → Звук → Громкость
//! приложений и устройства». **Недокументированный** механизм Windows (этап 7): фабрика WinRT
//! `Windows.Media.Internal.AudioPolicyConfig`. Порядок методов взят из открытых программ, которые
//! им пользуются (EarTrumpet, MIT), и проверен опытом на Windows 11 24H2 (тест `redirect_original`).
//!
//! Настройка **сохраняется Windows** для программы. Всё, что SoundBimba так меняет, она должна
//! уметь вернуть («по умолчанию» — пустое устройство). `ClearAllPersistedApplicationDefaultEndpoints`
//! (сбросить у всех программ) здесь нарочно нет.

use std::ffi::c_void;

use windows::core::{GUID, HRESULT, HSTRING};
use windows::Win32::Media::Audio::{eCommunications, eConsole, eMultimedia, eRender, EDataFlow, ERole};
use windows::Win32::System::WinRT::RoGetActivationFactory;

use crate::devices::ensure_com;

const CLASS: &str = "Windows.Media.Internal.AudioPolicyConfig";
/// Интерфейс фабрики с Windows 10 21H2 и в Windows 11.
const IID_FACTORY: GUID = GUID::from_u128(0xab3d4648_e242_459f_b02f_541c70306324);
/// Так Windows записывает устройство вывода в этой настройке.
const MMDEVAPI_PREFIX: &str = r"\\?\SWD#MMDEVAPI#";
const RENDER_INTERFACE: &str = "#{e6327cad-dcec-4949-ae8a-991e976a79d2}";

#[repr(C)]
struct Vtbl {
    // IUnknown + IInspectable.
    query_interface: usize,
    add_ref: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    get_iids: usize,
    get_runtime_class_name: usize,
    get_trust_level: usize,
    /// 19 методов, которые нам не нужны (громкость групп, «чат-приложение» и т.п.).
    _unused: [usize; 19],
    set_persisted_default: unsafe extern "system" fn(*mut c_void, u32, EDataFlow, ERole, *mut c_void) -> HRESULT,
    get_persisted_default: unsafe extern "system" fn(*mut c_void, u32, EDataFlow, ERole, *mut *mut c_void) -> HRESULT,
}

struct Factory(*mut c_void);

impl Drop for Factory {
    fn drop(&mut self) {
        unsafe { ((*(*(self.0 as *mut *const Vtbl))).release)(self.0) };
    }
}

impl Factory {
    fn new() -> windows::core::Result<Factory> {
        ensure_com()?;
        let mut p: *mut c_void = std::ptr::null_mut();
        unsafe {
            let name = HSTRING::from(CLASS);
            // RoGetActivationFactory с произвольным IID — через сырой указатель.
            let raw: windows::core::IInspectable = RoGetActivationFactory(&name)?;
            windows::core::Interface::query(&raw, &IID_FACTORY, &mut p).ok()?;
        }
        Ok(Factory(p))
    }

    fn vtbl(&self) -> &Vtbl {
        unsafe { &**(self.0 as *mut *const Vtbl) }
    }
}

/// Полное имя устройства вывода в этой настройке по id из MMDevice.
fn full_id(device_id: &str) -> String {
    format!("{MMDEVAPI_PREFIX}{device_id}{RENDER_INTERFACE}")
}

/// id MMDevice из полного имени (или как есть, если формат другой).
fn short_id(full: &str) -> String {
    full.strip_prefix(MMDEVAPI_PREFIX).and_then(|s| s.strip_suffix(RENDER_INTERFACE)).unwrap_or(full).to_string()
}

/// Куда сейчас направлен звук программы (`None` — «по умолчанию», как у всей системы).
pub fn output_of(pid: u32) -> windows::core::Result<Option<String>> {
    let f = Factory::new()?;
    let mut out: *mut c_void = std::ptr::null_mut();
    unsafe { (f.vtbl().get_persisted_default)(f.0, pid, eRender, eMultimedia, &mut out).ok()? };
    // Владение HSTRING переходит к нам.
    let s: HSTRING = unsafe { std::mem::transmute(out) };
    let s = s.to_string();
    Ok((!s.is_empty()).then(|| short_id(&s)))
}

/// Направить звук программы на устройство (`None` — вернуть «по умолчанию»). Для всех ролей.
pub fn set_output_of(pid: u32, device_id: Option<&str>) -> windows::core::Result<()> {
    let f = Factory::new()?;
    let id = HSTRING::from(device_id.map(full_id).unwrap_or_default());
    for role in [eConsole, eMultimedia, eCommunications] {
        let raw: *mut c_void = unsafe { std::mem::transmute_copy(&id) };
        unsafe { (f.vtbl().set_persisted_default)(f.0, pid, eRender, role, raw).ok()? };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_names_round_trip() {
        let id = "{0.0.0.00000000}.{b1493321-1151-4d3b-8f5a-2c89f1d9a03c}";
        assert_eq!(short_id(&full_id(id)), id);
        assert_eq!(short_id("strange"), "strange");
    }
}
