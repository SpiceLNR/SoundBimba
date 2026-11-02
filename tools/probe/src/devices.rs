// SPDX-License-Identifier: AGPL-3.0-or-later
//! Шаги 3, 4, 8: аудио-устройства, их форматы и периоды, поиск виртуальных кабелей.

use windows::core::{Result, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::{
    PKEY_DeviceInterface_FriendlyName, PKEY_Device_FriendlyName,
};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, EDataFlow, IAudioClient, IAudioClient3, IMMDevice,
    IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ};

pub use winaudio::cables::{cable_of, CableKind, CABLES};

use crate::format::Fmt;
use crate::report::Report;
use crate::rep;

/// Найденный кабель: его «динамик» (куда пишем) и «микрофон» (откуда читают другие программы).
pub struct Cable<'a> {
    pub kind: &'static CableKind,
    pub speaker: Option<&'a Device>,
    pub mic: Option<&'a Device>,
}

pub fn find_cables(devs: &[Device]) -> Vec<Cable<'_>> {
    CABLES
        .iter()
        .map(|kind| Cable {
            kind,
            speaker: devs.iter().find(|d| d.render && d.cable().is_some_and(|k| std::ptr::eq(k, kind))),
            mic: devs.iter().find(|d| !d.render && d.cable().is_some_and(|k| std::ptr::eq(k, kind))),
        })
        .filter(|c| c.speaker.is_some() || c.mic.is_some())
        .collect()
}

pub struct Device {
    pub dev: IMMDevice,
    pub id: String,
    pub name: String,
    pub adapter: String,
    pub render: bool,
    pub is_default: bool,
    pub fmt: Option<Fmt>,
    /// Период движка в 100-нс единицах: (обычный, минимальный).
    pub period: Option<(i64, i64)>,
    /// IAudioClient3, в кадрах: (обычный, шаг, минимальный, максимальный).
    pub engine: Option<(u32, u32, u32, u32)>,
    pub errors: Vec<String>,
}

impl Device {
    pub fn cable(&self) -> Option<&'static CableKind> {
        cable_of(&self.name, &self.adapter)
    }
}

pub fn pwstr_take(p: PWSTR) -> String {
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as _)) };
    s
}

fn prop(dev: &IMMDevice, key: &PROPERTYKEY) -> Result<String> {
    unsafe {
        let store = dev.OpenPropertyStore(STGM_READ)?;
        // PROPVARIANT сам вызывает PropVariantClear при удалении.
        let pv = store.GetValue(key)?;
        PropVariantToStringAlloc(&pv).map(pwstr_take)
    }
}

pub fn enumerator() -> Result<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

fn describe_errors(e: &windows_core::Error) -> String {
    format!("{} (код 0x{:08X})", e.message().trim(), e.code().0 as u32)
}

fn inspect(dev: IMMDevice, render: bool, default_id: &str) -> Result<Device> {
    let id = pwstr_take(unsafe { dev.GetId()? });
    let mut d = Device {
        name: prop(&dev, &PKEY_Device_FriendlyName).unwrap_or_else(|_| "?".into()),
        adapter: prop(&dev, &PKEY_DeviceInterface_FriendlyName).unwrap_or_default(),
        is_default: id == default_id,
        id,
        dev,
        render,
        fmt: None,
        period: None,
        engine: None,
        errors: Vec::new(),
    };
    unsafe {
        match d.dev.Activate::<IAudioClient>(CLSCTX_ALL, None) {
            Ok(client) => match client.GetMixFormat() {
                Ok(mix) => {
                    d.fmt = Some(Fmt::from_ptr(mix));
                    let (mut def, mut min) = (0i64, 0i64);
                    match client.GetDevicePeriod(Some(&mut def), Some(&mut min)) {
                        Ok(()) => d.period = Some((def, min)),
                        Err(e) => d.errors.push(format!("период: {}", describe_errors(&e))),
                    }
                    match d.dev.Activate::<IAudioClient3>(CLSCTX_ALL, None) {
                        Ok(c3) => {
                            let (mut a, mut b, mut c, mut x) = (0u32, 0u32, 0u32, 0u32);
                            match c3.GetSharedModeEnginePeriod(mix, &mut a, &mut b, &mut c, &mut x) {
                                Ok(()) => d.engine = Some((a, b, c, x)),
                                Err(e) => d.errors.push(format!("IAudioClient3: {}", describe_errors(&e))),
                            }
                        }
                        Err(e) => d.errors.push(format!("IAudioClient3 нет: {}", describe_errors(&e))),
                    }
                    CoTaskMemFree(Some(mix as _));
                }
                Err(e) => d.errors.push(format!("формат: {}", describe_errors(&e))),
            },
            Err(e) => d.errors.push(format!("IAudioClient: {}", describe_errors(&e))),
        }
    }
    Ok(d)
}

fn list_flow(en: &IMMDeviceEnumerator, flow: EDataFlow) -> Result<Vec<Device>> {
    let default_id = unsafe { en.GetDefaultAudioEndpoint(flow, eConsole) }
        .and_then(|d| unsafe { d.GetId() }.map(pwstr_take))
        .unwrap_or_default();
    let coll = unsafe { en.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)? };
    let mut out = Vec::new();
    for i in 0..unsafe { coll.GetCount()? } {
        out.push(inspect(unsafe { coll.Item(i)? }, flow == eRender, &default_id)?);
    }
    Ok(out)
}

pub fn list_all(en: &IMMDeviceEnumerator) -> Result<Vec<Device>> {
    let mut v = list_flow(en, eRender)?;
    v.extend(list_flow(en, eCapture)?);
    Ok(v)
}

fn ms(hns: i64) -> String {
    format!("{:.2} мс", hns as f64 / 10_000.0)
}

fn frames_ms(frames: u32, rate: u32) -> String {
    format!("{frames} кадров = {:.2} мс", frames as f64 * 1000.0 / rate.max(1) as f64)
}

fn print_device(r: &mut Report, d: &Device) {
    let mut marks = Vec::new();
    if d.is_default {
        marks.push("ПО УМОЛЧАНИЮ".to_string());
    }
    if let Some(k) = d.cable() {
        marks.push(format!("ВИРТУАЛЬНЫЙ КАБЕЛЬ {}", k.label));
    }
    let marks = if marks.is_empty() { String::new() } else { format!("  [{}]", marks.join(", ")) };
    rep!(r, "  • {}{marks}", d.name);
    if !d.adapter.is_empty() {
        rep!(r, "      адаптер: {}", d.adapter);
    }
    rep!(r, "      id: {}", d.id);
    if let Some(f) = &d.fmt {
        rep!(r, "      формат смешивания: {}", f.describe());
    }
    if let Some((def, min)) = d.period {
        rep!(r, "      период: обычный {}, минимальный {}", ms(def), ms(min));
    }
    if let (Some((def, fund, min, max)), Some(f)) = (d.engine, &d.fmt) {
        rep!(
            r,
            "      IAudioClient3: обычный {}, минимальный {}, максимальный {}, шаг {fund}",
            frames_ms(def, f.rate),
            frames_ms(min, f.rate),
            frames_ms(max, f.rate)
        );
    }
    for e in &d.errors {
        rep!(r, "      не удалось: {e}");
    }
}

pub fn report_devices(r: &mut Report, devs: &[Device]) {
    r.section("3. Аудио-устройства");
    rep!(r, " Вывод (динамики, наушники):");
    devs.iter().filter(|d| d.render).for_each(|d| print_device(r, d));
    rep!(r, " Ввод (микрофоны):");
    devs.iter().filter(|d| !d.render).for_each(|d| print_device(r, d));
}

pub fn report_cables(r: &mut Report, cables: &[Cable]) {
    r.section("4. Виртуальный кабель");
    if cables.is_empty() {
        rep!(r, "  ИТОГ: виртуальный кабель НЕ найден. Рекомендуем VB-CABLE: https://vb-audio.com/Cable/");
        return;
    }
    let describe = |d: Option<&Device>| match d {
        Some(d) => {
            let f = d.fmt.map(|f| f.describe()).unwrap_or_else(|| "формат неизвестен".into());
            format!("{} — {f}", d.name)
        }
        None => "НЕ НАЙДЕН".into(),
    };
    for c in cables {
        rep!(r, "  {}:", c.kind.label);
        rep!(r, "    динамик (сюда пишет SoundBimba): {}", describe(c.speaker));
        rep!(r, "    микрофон (его выбирают в Discord/OBS): {}", describe(c.mic));
        if let Some(note) = c.kind.note {
            rep!(r, "    примечание: {note}");
        }
        if c.speaker.is_none() || c.mic.is_none() {
            rep!(r, "    ИТОГ: найдено только одно из двух устройств — кабель установлен не полностью?");
        } else {
            rep!(r, "    ИТОГ: кабель найден (оба устройства)");
        }
    }
}

pub fn report_latency(r: &mut Report, devs: &[Device]) {
    r.section("8. Задержка буфера (ориентировочно)");
    rep!(r, "  Это только размер буфера звуковой системы, а не полная задержка.");
    for d in devs.iter().filter(|d| d.is_default) {
        let kind = if d.render { "вывод" } else { "ввод" };
        let shared = d.period.map(|(def, _)| ms(def)).unwrap_or_else(|| "не удалось измерить".into());
        let low = match (d.engine, &d.fmt) {
            (Some((_, _, min, _)), Some(f)) => frames_ms(min, f.rate),
            _ => "недоступно".into(),
        };
        rep!(r, "  {kind} «{}»: shared {shared}; shared-lowlatency (минимум IAudioClient3) {low}", d.name);
    }
}
