// SPDX-License-Identifier: AGPL-3.0-or-later
//! Шаг 6: захват звука одного процесса через process loopback (вместе с дочерними процессами).
//! Образец — пример Microsoft ApplicationLoopback.

use std::mem::ManuallyDrop;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use windows::core::{implement, Interface, Ref, Result, HRESULT, IUnknown, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, E_FAIL, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl,
    IAudioCaptureClient, IAudioClient, AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY,
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, AUDIOCLIENT_ACTIVATION_PARAMS,
    AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
    AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
    VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX,
};
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0};
use windows::Win32::System::Com::BLOB;
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;

use crate::format::{float_format, pcm16_format, Fmt};

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Activated(Arc<(Mutex<bool>, Condvar)>);

impl IActivateAudioInterfaceCompletionHandler_Impl for Activated_Impl {
    fn ActivateCompleted(&self, _op: Ref<IActivateAudioInterfaceAsyncOperation>) -> Result<()> {
        let (lock, cv) = &*self.0;
        *lock.lock().unwrap() = true;
        cv.notify_all();
        Ok(())
    }
}

/// Создаёт IAudioClient, который слышит только процесс `pid` и его дочерние процессы.
fn activate(pid: u32) -> Result<IAudioClient> {
    let params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    let mut inner = PROPVARIANT_0_0 { vt: VT_BLOB, ..Default::default() };
    inner.Anonymous.blob = BLOB {
        cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
        pBlobData: &params as *const _ as *mut u8,
    };
    // PROPVARIANT в крейте `windows` при удалении вызывает PropVariantClear, а тот освободил бы
    // `params` из стека (порча кучи 0xC0000374). Поэтому — ManuallyDrop.
    let pv = ManuallyDrop::new(PROPVARIANT { Anonymous: PROPVARIANT_0 { Anonymous: ManuallyDrop::new(inner) } });

    let sync = Arc::new((Mutex::new(false), Condvar::new()));
    let handler: IActivateAudioInterfaceCompletionHandler = Activated(sync.clone()).into();
    unsafe {
        let op = ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&*pv),
            &handler,
        )?;
        let (lock, cv) = &*sync;
        let (done, _) = cv
            .wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(10), |d| !*d)
            .unwrap();
        if !*done {
            return Err(windows::core::Error::new(E_FAIL, "Windows не ответила за 10 секунд"));
        }
        let mut hr = HRESULT(0);
        let mut unk: Option<IUnknown> = None;
        op.GetActivateResult(&mut hr, &mut unk)?;
        hr.ok()?;
        unk.ok_or_else(|| windows::core::Error::from(E_FAIL))?.cast()
    }
}

pub struct EventHandle(pub HANDLE);

impl EventHandle {
    pub fn new() -> Result<Self> {
        Ok(Self(unsafe { CreateEventW(None, false, false, PCWSTR::null())? }))
    }
    /// true — событие пришло, false — истекло время ожидания.
    pub fn wait(&self, ms: u32) -> bool {
        unsafe { WaitForSingleObject(self.0, ms) == WAIT_OBJECT_0 }
    }
}

impl Drop for EventHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub struct Captured {
    pub fmt: Fmt,
    pub fmt_note: &'static str,
    pub samples: Vec<f32>,
    pub packets: u32,
    pub silent_packets: u32,
    pub discontinuities: u32,
    pub timeouts: u32,
}

impl Captured {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.fmt.channels.max(1) as usize
    }
    pub fn peak(&self) -> f32 {
        self.samples.iter().fold(0.0, |m, s| m.max(s.abs()))
    }
}

pub fn capture_process(pid: u32, seconds: u32) -> Result<Captured> {
    let client = activate(pid)?;
    // У process loopback нет GetMixFormat, формат задаём сами; при отказе — 16 бит, как у Microsoft.
    let flags = AUDCLNT_STREAMFLAGS_LOOPBACK
        | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    let float = float_format(48000, 2);
    let (client, wf, fmt_note): (IAudioClient, WAVEFORMATEX, &'static str) =
        match unsafe { client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 200_000, 0, &float, None) } {
            Ok(()) => (client, float, "float 48 кГц принят"),
            Err(_) => {
                // После неудачного Initialize клиент использовать нельзя — берём новый.
                let client = activate(pid)?;
                let pcm = pcm16_format(44100, 2);
                unsafe { client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 200_000, 0, &pcm, None)? };
                (client, pcm, "float не принят, взят запасной PCM 16 бит 44.1 кГц")
            }
        };
    let fmt = unsafe { Fmt::from_ptr(&wf) };

    let event = EventHandle::new()?;
    let mut out = Captured {
        fmt,
        fmt_note,
        samples: Vec::with_capacity(fmt.rate as usize * fmt.channels as usize * (seconds as usize + 1)),
        packets: 0,
        silent_packets: 0,
        discontinuities: 0,
        timeouts: 0,
    };
    unsafe {
        client.SetEventHandle(event.0)?;
        let cap: IAudioCaptureClient = client.GetService()?;
        client.Start()?;
        let end = Instant::now() + Duration::from_secs(seconds as u64);
        let result = (|| -> Result<()> {
            while Instant::now() < end {
                if !event.wait(500) {
                    out.timeouts += 1;
                }
                drain(&cap, &fmt, &mut out)?;
            }
            Ok(())
        })();
        let _ = client.Stop();
        result?;
    }
    Ok(out)
}

fn drain(cap: &IAudioCaptureClient, fmt: &Fmt, out: &mut Captured) -> Result<()> {
    unsafe {
        while cap.GetNextPacketSize()? > 0 {
            let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
            cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            out.packets += 1;
            let n = frames as usize * fmt.block_align as usize;
            if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                out.silent_packets += 1;
                out.samples.extend(std::iter::repeat_n(0.0, frames as usize * fmt.channels as usize));
            } else {
                fmt.push_f32(std::slice::from_raw_parts(data, n), &mut out.samples);
            }
            if flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 {
                out.discontinuities += 1;
            }
            cap.ReleaseBuffer(frames)?;
        }
    }
    Ok(())
}
