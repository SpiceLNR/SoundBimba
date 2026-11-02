// SPDX-License-Identifier: AGPL-3.0-or-later
//! Замер задержки через виртуальный кабель тем же кодом, что у кнопки «Проверить задержку».
//! `cargo test -p winaudio --test cable_latency -- --ignored --nocapture`

use winaudio::devices;
use winaudio::latency::measure_cable;

#[test]
#[ignore = "нужен установленный VB-CABLE"]
fn latency_through_the_cable() {
    let devs = devices::list().expect("device list");
    let spk = devices::primary_cable_render(&devs).expect("кабель не найден");
    let mic = devices::primary_cable_capture(&devs).expect("кабель не найден");
    for (render, capture) in [(15.0, 15.0), (8.0, 12.0)] {
        match measure_cable(&spk.id, &mic.id, render, capture) {
            Ok(ms) => println!("запас вывод {render} / захват {capture} мс: задержка ≈ {ms:.1} мс"),
            Err(e) => println!("запас вывод {render} / захват {capture} мс: не удалось — {e}"),
        }
    }
}
