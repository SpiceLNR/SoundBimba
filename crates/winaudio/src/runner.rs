// SPDX-License-Identifier: AGPL-3.0-or-later
//! Поток движка: каждые 5 мс обрабатывает блок графа. Часы — высокоточный таймер Windows,
//! приоритет — системная служба мультимедиа (MMCSS, задача «Pro Audio»).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use engine::{Graph, BLOCK, RATE};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateWaitableTimerExW, SetWaitableTimer,
    WaitForSingleObject, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE, TIMER_ALL_ACCESS,
};

/// Если поток отстал больше чем на столько, он не догоняет блоками, а начинает отсчёт заново.
const MAX_LAG: Duration = Duration::from_millis(50);

pub struct Runner {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Graph>>,
    /// Сколько раз поток не успел к сроку и начал отсчёт заново.
    pub late: Arc<AtomicU32>,
}

struct Timer(HANDLE);

impl Timer {
    fn new() -> Option<Timer> {
        unsafe {
            CreateWaitableTimerExW(None, PCWSTR::null(), CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS.0)
                .ok()
                .map(Timer)
        }
    }

    fn sleep(&self, d: Duration) {
        let due = -((d.as_nanos() / 100) as i64).max(1); // отрицательное — «через столько» (100 нс)
        unsafe {
            if SetWaitableTimer(self.0, &due, 0, None, None, false).is_ok() {
                WaitForSingleObject(self.0, INFINITE);
            } else {
                std::thread::sleep(d);
            }
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub fn start(mut graph: Graph) -> Runner {
    let stop = Arc::new(AtomicBool::new(false));
    let late = Arc::new(AtomicU32::new(0));
    let (s, l) = (stop.clone(), late.clone());
    let thread = std::thread::Builder::new()
        .name("sb-engine".into())
        .spawn(move || {
            let mut task = 0u32;
            let mmcss = unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task) }.ok();
            let timer = Timer::new();
            let block = Duration::from_secs_f64(BLOCK as f64 / RATE as f64);
            let mut next = Instant::now();
            while !s.load(Relaxed) {
                graph.process();
                next += block;
                let now = Instant::now();
                if now > next + MAX_LAG {
                    next = now;
                    l.fetch_add(1, Relaxed);
                } else if next > now {
                    match &timer {
                        Some(t) => t.sleep(next - now),
                        None => std::thread::sleep(next - now),
                    }
                }
            }
            if let Some(h) = mmcss {
                let _ = unsafe { AvRevertMmThreadCharacteristics(h) };
            }
            graph
        })
        .expect("cannot start the audio engine thread");
    Runner { stop, thread: Some(thread), late }
}

impl Drop for Runner {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join(); // граф освобождается здесь, вне потока реального времени
        }
    }
}
