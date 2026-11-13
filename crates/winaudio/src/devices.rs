// SPDX-License-Identifier: AGPL-3.0-or-later
//! Список активных звуковых устройств (вывод и ввод) с отметкой виртуальных кабелей.

use std::collections::HashMap;
use std::sync::Mutex;

use windows::core::{Result, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::{
    PKEY_DeviceInterface_FriendlyName, PKEY_Device_DeviceDesc, PKEY_Device_FriendlyName,
};
use windows::Win32::Foundation::{PROPERTYKEY, RPC_E_CHANGED_MODE};
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, EDataFlow, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};

use crate::cables::{cable_of, CableKind};

#[derive(Clone)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub adapter: String,
    pub render: bool,
    pub is_default: bool,
    pub cable: Option<&'static CableKind>,
}

/// Запускает COM в этом потоке, если он ещё не запущен (в любом режиме).
pub fn ensure_com() -> Result<()> {
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if hr == RPC_E_CHANGED_MODE {
        return Ok(());
    }
    hr.ok()
}

fn take(p: PWSTR) -> String {
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as _)) };
    s
}

fn prop(dev: &IMMDevice, key: &PROPERTYKEY) -> Result<String> {
    unsafe {
        let store = dev.OpenPropertyStore(STGM_READ)?;
        // PROPVARIANT сам вызывает PropVariantClear при удалении.
        let pv = store.GetValue(key)?;
        PropVariantToStringAlloc(&pv).map(take)
    }
}

/// Последние прочитанные названия устройств (id → название, адаптер). Windows иногда на миг не
/// отдаёт название (смена устройств, выход из сна) — тогда берём запомненное: иначе виртуальный
/// кабель, который узнаём по названию, «пропадает» (так было у тестировщика на Windows 10).
static NAMES: Mutex<Option<HashMap<String, (String, String)>>> = Mutex::new(None);

fn names_of(dev: &IMMDevice, id: &str) -> (String, String) {
    let name = prop(dev, &PKEY_Device_FriendlyName).or_else(|_| prop(dev, &PKEY_Device_DeviceDesc)).ok();
    let adapter = prop(dev, &PKEY_DeviceInterface_FriendlyName).ok();
    let mut cache = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    let known = cache.get(id).cloned();
    let name = name.filter(|n| !n.is_empty()).or_else(|| known.as_ref().map(|k| k.0.clone())).unwrap_or_else(|| "?".into());
    let adapter = adapter.filter(|a| !a.is_empty()).or_else(|| known.map(|k| k.1)).unwrap_or_default();
    if name != "?" {
        cache.insert(id.to_string(), (name.clone(), adapter.clone()));
    }
    (name, adapter)
}

fn list_flow(en: &IMMDeviceEnumerator, flow: EDataFlow, out: &mut Vec<DeviceInfo>) -> Result<()> {
    let default_id = unsafe { en.GetDefaultAudioEndpoint(flow, eConsole) }
        .and_then(|d| unsafe { d.GetId() }.map(take))
        .unwrap_or_default();
    let coll = unsafe { en.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)? };
    for i in 0..unsafe { coll.GetCount()? } {
        // Одно устройство не прочиталось — пропускаем его, а не весь список.
        let Ok(dev) = (unsafe { coll.Item(i) }) else { continue };
        let Ok(id) = (unsafe { dev.GetId() }).map(take) else { continue };
        let (name, adapter) = names_of(&dev, &id);
        out.push(DeviceInfo {
            is_default: id == default_id,
            cable: cable_of(&name, &adapter),
            id,
            name,
            adapter,
            render: flow == eRender,
        });
    }
    Ok(())
}

/// Все активные устройства: сначала вывод, потом ввод.
pub fn list() -> Result<Vec<DeviceInfo>> {
    ensure_com()?;
    let en: IMMDeviceEnumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let mut out = Vec::new();
    list_flow(&en, eRender, &mut out)?;
    list_flow(&en, eCapture, &mut out)?;
    Ok(out)
}

/// Главный «динамик» виртуального кабеля, куда SoundBimba отправляет звук для Discord и OBS.
/// У VB-CABLE есть второй вход «CABLE In 16ch» — его берём, только если основного нет.
pub fn primary_cable_render(devs: &[DeviceInfo]) -> Option<&DeviceInfo> {
    let cables = || devs.iter().filter(|d| d.render && d.cable.is_some());
    cables().find(|d| !d.name.to_lowercase().contains("16ch")).or_else(|| cables().next())
}

/// «Микрофон» основного виртуального кабеля (его выбирают в Discord и OBS).
pub fn primary_cable_capture(devs: &[DeviceInfo]) -> Option<&DeviceInfo> {
    let cables = || devs.iter().filter(|d| !d.render && d.cable.is_some());
    cables().find(|d| !d.name.to_lowercase().contains("16ch")).or_else(|| cables().next())
}

/// Устройство по умолчанию (вывод или ввод).
pub fn default_device(devs: &[DeviceInfo], render: bool) -> Option<&DeviceInfo> {
    devs.iter().find(|d| d.render == render && d.is_default)
}
