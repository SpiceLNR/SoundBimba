// SPDX-License-Identifier: AGPL-3.0-or-later
//! Опыт для этапа 7: можно ли заглушить «оригинал» программы (громкость её сессии в микшере
//! Windows), а копию через захват процесса оставить? Тестовая программа (этот же файл, запущенный
//! вторым процессом) играет тон в VB-CABLE — в колонках ничего не слышно. Меряем две вещи: что
//! снимает захват процесса и что реально доходит до устройства (микрофон кабеля).
//! Запуск вручную: `cargo test -p winaudio --test mute_original -- --ignored --nocapture --test-threads 1`

use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use engine::{RingReader, BLOCK, BLOCK_SAMPLES, RATE};
use windows::core::Interface;
use windows::Win32::Media::Audio::{
    eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use winaudio::devices;
use winaudio::streams::{open_capture, open_render, Source};

const CHILD_ENV: &str = "SB_TONE_CHILD";

/// Вторая половина опыта: этот же тест в отдельном процессе играет тон на устройство из переменной.
#[test]
#[ignore = "запускается опытом mute_then_capture"]
fn tone_child() {
    let Ok(device) = std::env::var(CHILD_ENV) else { return };
    let (_out, mut writer) = open_render(&device, 20.0).expect("render");
    let mut block = vec![0.0f32; BLOCK_SAMPLES];
    let mut phase = 0usize;
    let start = Instant::now();
    let blocks = 14 * RATE as usize / BLOCK;
    for b in 0..blocks {
        for f in 0..BLOCK {
            let v = 0.3 * (TAU * 440.0 * (phase + f) as f32 / RATE as f32).sin();
            block[f * 2] = v;
            block[f * 2 + 1] = v;
        }
        phase += BLOCK;
        writer.write(&block);
        let due = start + Duration::from_secs_f64((b + 1) as f64 * BLOCK as f64 / RATE as f64);
        if let Some(w) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(w);
        }
    }
}

fn tone_amplitude(x: &[f32]) -> f32 {
    let coeff = 2.0 * (TAU * 440.0 / RATE as f32).cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &v in x {
        let s0 = v + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    2.0 * (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt() / x.len() as f32
}

/// Слушает оба источника `seconds` секунд и возвращает уровень тона 440 Гц (последняя секунда).
fn listen(copy: &mut RingReader, device: &mut RingReader, seconds: f32) -> (f32, f32) {
    let blocks = (seconds * RATE as f32 / BLOCK as f32) as usize;
    let (mut a, mut b) = (Vec::new(), Vec::new());
    let mut buf = vec![0.0f32; BLOCK_SAMPLES];
    let start = Instant::now();
    for i in 0..blocks {
        copy.read(&mut buf);
        a.extend(buf.iter().step_by(2).copied());
        device.read(&mut buf);
        b.extend(buf.iter().step_by(2).copied());
        let due = start + Duration::from_secs_f64((i + 1) as f64 * BLOCK as f64 / RATE as f64);
        if let Some(w) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(w);
        }
    }
    let tail = |x: &[f32]| tone_amplitude(&x[x.len() - RATE as usize..]);
    (tail(&a), tail(&b))
}

/// Громкость сессии программы `pid` на устройстве `device_id` (как ползунок в микшере Windows).
fn session_volume(device_id: &str, pid: u32) -> ISimpleAudioVolume {
    devices::ensure_com().unwrap();
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).unwrap();
        let devs = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE).unwrap();
        for i in 0..devs.GetCount().unwrap() {
            let dev = devs.Item(i).unwrap();
            if dev.GetId().unwrap().to_string().unwrap() != device_id {
                continue;
            }
            let mgr: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None).unwrap();
            let sessions = mgr.GetSessionEnumerator().unwrap();
            for s in 0..sessions.GetCount().unwrap() {
                let ctl = sessions.GetSession(s).unwrap();
                let ctl2: IAudioSessionControl2 = ctl.cast().unwrap();
                if ctl2.GetProcessId().unwrap_or(0) == pid {
                    return ctl.cast().unwrap();
                }
            }
        }
    }
    panic!("у тестовой программы нет аудиосессии на кабеле");
}

#[test]
#[ignore = "нужен VB-CABLE"]
fn mute_then_capture() {
    let devs = devices::list().expect("devices");
    let cable = devices::primary_cable_render(&devs).expect("нет VB-CABLE").id.clone();
    let cable_mic = devices::primary_cable_capture(&devs).expect("нет CABLE Output").id.clone();

    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["tone_child", "--exact", "--ignored", "--nocapture"])
        .env(CHILD_ENV, &cable)
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("запуск тестовой программы");
    std::thread::sleep(Duration::from_millis(1500));

    let (_copy_stream, mut copy) = open_capture(Source::Process(child.id()), 60.0).expect("захват процесса");
    let (_dev_stream, mut device) = open_capture(Source::Device(cable_mic), 60.0).expect("микрофон кабеля");
    let vol = session_volume(&cable, child.id());

    let (c, d) = listen(&mut copy, &mut device, 3.0);
    println!("1. как обычно:           копия {c:.3}, на устройстве {d:.3}");
    unsafe { vol.SetMute(true, std::ptr::null()).unwrap() };
    let (c, d) = listen(&mut copy, &mut device, 3.0);
    println!("2. «выкл. звук» в микшере: копия {c:.3}, на устройстве {d:.3}");
    unsafe {
        vol.SetMute(false, std::ptr::null()).unwrap();
        vol.SetMasterVolume(0.0, std::ptr::null()).unwrap();
    }
    let (c, d) = listen(&mut copy, &mut device, 3.0);
    println!("3. громкость 0 в микшере: копия {c:.3}, на устройстве {d:.3}");
    unsafe { vol.SetMasterVolume(1.0, std::ptr::null()).unwrap() };
    let _ = child.kill();
    let _ = child.wait();
}
