// SPDX-License-Identifier: AGPL-3.0-or-later
//! Захват звука одного процесса (process loopback, вместе с дочерними процессами) и событие
//! Windows для потоков звука. Образец — пример Microsoft ApplicationLoopback.

use std::mem::ManuallyDrop;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use windows::core::{implement, Interface, Ref, Result, HRESULT, IUnknown, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, E_FAIL, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl, IAudioClient,
    AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
    AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
    VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
};
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0};
use windows::Win32::System::Com::BLOB;
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;

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
pub fn activate_process_loopback(pid: u32) -> Result<IAudioClient> {
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
