// SPDX-License-Identifier: AGPL-3.0-or-later
//! Список активных звуковых устройств (вывод и ввод) с отметкой виртуальных кабелей.

use windows::core::{Result, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::{
    PKEY_DeviceInterface_FriendlyName, PKEY_Device_FriendlyName,
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

fn list_flow(en: &IMMDeviceEnumerator, flow: EDataFlow, out: &mut Vec<DeviceInfo>) -> Result<()> {
    let default_id = unsafe { en.GetDefaultAudioEndpoint(flow, eConsole) }
        .and_then(|d| unsafe { d.GetId() }.map(take))
        .unwrap_or_default();
    let coll = unsafe { en.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)? };
    for i in 0..unsafe { coll.GetCount()? } {
        let dev = unsafe { coll.Item(i)? };
        let id = take(unsafe { dev.GetId()? });
        let name = prop(&dev, &PKEY_Device_FriendlyName).unwrap_or_else(|_| "?".into());
        let adapter = prop(&dev, &PKEY_DeviceInterface_FriendlyName).unwrap_or_default();
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
