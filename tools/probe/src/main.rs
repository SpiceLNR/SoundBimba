// SPDX-License-Identifier: AGPL-3.0-or-later
//! SoundBimba, этап 0: «проба». Смотрит, что умеет звук на этом компьютере.
//! Ничего не меняет в системе и ничего не отправляет в сеть.
//! Результат дописывается в probe-report.txt рядом с программой.

mod capture;
mod devices;
mod format;
mod looptest;
mod report;
mod sessions;
mod sysinfo;
mod wav;

use std::io::{BufRead, IsTerminal};
use std::path::PathBuf;

use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

use devices::Device;
use report::Report;

const HELP: &str = "\
probe.exe — проверка звука для SoundBimba (этап 0)

Без параметров: полная проверка, затем (по желанию) запись звука программы
и проверка виртуального микрофона. Отчёт: probe-report.txt рядом с программой.

  --pid <номер>     записать звук процесса с этим PID
  --exe <имя.exe>   записать звук программы по имени файла (например, chrome.exe)
  --seconds <N>     длительность записи, 1–60 секунд (по умолчанию 10)
  --loop-test       только проверка виртуального кабеля (динамик → микрофон)
  --no-pause        не ждать Enter в конце
";

#[derive(Default)]
struct Args {
    pid: Option<u32>,
    exe: Option<String>,
    seconds: u32,
    loop_test: bool,
    no_pause: bool,
    help: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args { seconds: 10, ..Default::default() };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("после {name} нужно значение"));
        match arg.as_str() {
            "--pid" => a.pid = Some(value("--pid")?.parse().map_err(|_| "PID должен быть числом")?),
            "--exe" => a.exe = Some(value("--exe")?),
            "--seconds" => {
                a.seconds = value("--seconds")?.parse().map_err(|_| "--seconds должно быть числом")?;
                if !(1..=60).contains(&a.seconds) {
                    return Err("--seconds: от 1 до 60".into());
                }
            }
            "--loop-test" => a.loop_test = true,
            "--no-pause" => a.no_pause = true,
            "--help" | "-h" | "/?" => a.help = true,
            other => return Err(format!("неизвестный параметр «{other}»")),
        }
    }
    Ok(a)
}

/// Папка, где лежит probe.exe: туда пишем отчёт и WAV-файлы.
fn out_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn ask(prompt: &str) -> Option<String> {
    if !std::io::stdin().is_terminal() {
        return None;
    }
    println!("{prompt}");
    let mut s = String::new();
    std::io::stdin().lock().read_line(&mut s).ok()?;
    Some(s.trim().to_string())
}

fn load_devices(r: &mut Report) -> Vec<Device> {
    let res = devices::enumerator().and_then(|en| devices::list_all(&en));
    match res {
        Ok(d) => d,
        Err(e) => {
            r.error("список устройств", &e);
            Vec::new()
        }
    }
}

fn run_capture(r: &mut Report, pid: u32, seconds: u32) {
    let exe = sessions::process_name(pid);
    r.section(&format!(
        "6. Запись звука процесса {} (PID {pid}), {seconds} с",
        exe.as_deref().unwrap_or("?")
    ));
    if exe.is_none() {
        rep!(r, "  Процесс с таким PID не найден или нет доступа. Запись всё равно пробуем.");
    }
    rep!(r, "  Идёт запись… Пусть программа в это время играет звук.");
    match capture::capture_process(pid, seconds) {
        Ok(c) => {
            let peak = c.peak();
            rep!(r, "  Формат: {} ({})", c.fmt.describe(), c.fmt_note);
            rep!(r, "  Получено кадров: {} (ожидалось около {})", c.frames(), c.fmt.rate * seconds);
            rep!(r, "  Пакетов: {}, из них пустых (тишина): {}", c.packets, c.silent_packets);
            rep!(r, "  Пропуски (discontinuity): {} (один в самом начале — норма)", c.discontinuities);
            rep!(r, "  Ожиданий без данных (по 0.5 с): {}", c.timeouts);
            let db = if peak > 0.0 { format!("{:.1} дБ", 20.0 * peak.log10()) } else { "−∞".into() };
            rep!(r, "  Максимальный уровень: {peak:.3} ({db})");
            if peak < 0.001 {
                rep!(r, "  ИТОГ: записана тишина. Программа молчала, или её звук не удалось захватить.");
            } else {
                rep!(r, "  ИТОГ: звук записан.");
            }
            let name = format!("capture-{}-{pid}.wav", exe.as_deref().unwrap_or("process").trim_end_matches(".exe"));
            let path = out_dir().join(&name);
            match wav::write_f32(&path, c.fmt.rate, c.fmt.channels, &c.samples) {
                Ok(()) => rep!(r, "  Файл для прослушивания: {}", path.display()),
                Err(e) => rep!(r, "  Не удалось сохранить WAV: {e}"),
            }
        }
        Err(e) => r.error("захват процесса", &e),
    }
}

fn run_loop_test(r: &mut Report, devs: &[Device]) {
    r.section("7. Проверка связки «динамик кабеля → микрофон кабеля»");
    let cables = devices::find_cables(devs);
    let full: Vec<_> = cables.iter().filter_map(|c| Some((c.kind, c.speaker?, c.mic?))).collect();
    if full.is_empty() {
        rep!(r, "  Пропущено: виртуальный кабель не найден (нужны оба устройства).");
    }
    for (kind, spk, mic) in full {
        loop_test_one(r, kind.label, spk, mic);
    }
}

fn loop_test_one(r: &mut Report, label: &str, spk: &Device, mic: &Device) {
    rep!(r, "  Кабель: {label}");
    rep!(r, "  Динамик: {}", spk.name);
    rep!(r, "  Микрофон: {}", mic.name);
    rep!(r, "  Играю тон {} Гц {} с и записываю с микрофона…", looptest::FREQ, looptest::TONE);
    match looptest::run(&spk.id, &mic.dev) {
        Ok(res) => {
            let a = looptest::analyze(&res);
            rep!(r, "  Формат динамика: {}", res.render.fmt.describe());
            rep!(r, "  Формат микрофона: {}", res.mic_fmt.describe());
            rep!(r, "  Записано кадров: {}; пустых пакетов: {}; пропусков: {}; задержек вывода: {}",
                res.mono.len(), res.silent_packets, res.discontinuities, res.render.underruns);
            rep!(r, "  Максимальный уровень на микрофоне: {:.3} (отправляли 0.500)", a.peak);
            if a.reached {
                rep!(r, "  ИТОГ: тон ДОШЁЛ до виртуального микрофона.");
                match a.freq {
                    Some(f) => rep!(r, "  Частота в записи: ≈ {f:.0} Гц (ожидалось {})", looptest::FREQ),
                    None => rep!(r, "  Частоту измерить не удалось (слишком короткая запись)."),
                }
                match a.latency_ms {
                    Some(ms) => rep!(r, "  Задержка «динамик → микрофон»: ≈ {ms:.1} мс"),
                    None => rep!(r, "  Задержку измерить не удалось (нет отметок времени)."),
                }
            } else if a.peak > 0.01 {
                rep!(r, "  ИТОГ: тон НЕ дошёл — на микрофоне есть звук, но это не наш тон {} Гц.", looptest::FREQ);
            } else {
                rep!(r, "  ИТОГ: тон НЕ дошёл — на микрофоне тишина.");
            }
            let file: String = label.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
            let path = out_dir().join(format!("loop-test-{file}.wav"));
            match wav::write_f32(&path, res.mic_fmt.rate, res.mic_fmt.channels, &res.all) {
                Ok(()) => rep!(r, "  Запись с микрофона: {}", path.display()),
                Err(e) => rep!(r, "  Не удалось сохранить WAV: {e}"),
            }
        }
        Err(e) => r.error("проверка связки", &e),
    }
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Ошибка: {e}\n\n{HELP}");
            std::process::exit(2);
        }
    };
    if args.help {
        print!("{HELP}");
        return;
    }

    let mut r = Report::default();
    rep!(r, "==================== SoundBimba probe {} ====================", env!("CARGO_PKG_VERSION"));
    rep!(r, "Параметры: {}", std::env::args().skip(1).collect::<Vec<_>>().join(" "));

    if let Err(e) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
        r.error("запуск COM", &e);
    }

    let capture_only = args.pid.is_some() || args.exe.is_some();
    if capture_only {
        let pid = args.pid.or_else(|| args.exe.as_deref().and_then(sessions::find_pid_by_exe));
        match pid {
            Some(pid) => run_capture(&mut r, pid, args.seconds),
            None => rep!(r, "Программа «{}» не запущена.", args.exe.as_deref().unwrap_or("")),
        }
    } else if args.loop_test {
        let devs = load_devices(&mut r);
        devices::report_cables(&mut r, &devices::find_cables(&devs));
        run_loop_test(&mut r, &devs);
    } else {
        sysinfo::system(&mut r);
        sysinfo::webview2(&mut r);
        let devs = load_devices(&mut r);
        devices::report_devices(&mut r, &devs);
        devices::report_cables(&mut r, &devices::find_cables(&devs));
        let (sess, errors) = sessions::list(&devs);
        sessions::report(&mut r, &sess, &errors);
        devices::report_latency(&mut r, &devs);

        let list = sessions::capturable(&sess);
        if !list.is_empty() {
            println!();
            for (i, s) in list.iter().enumerate() {
                println!("  {}) {} (PID {})", i + 1, s.exe, s.pid);
            }
            let prompt = format!(
                "Записать {} с звука программы? Введите номер из списка и нажмите Enter (просто Enter — пропустить):",
                args.seconds
            );
            if let Some(n) = ask(&prompt).and_then(|s| s.parse::<usize>().ok()) {
                if let Some(s) = list.get(n.wrapping_sub(1)) {
                    run_capture(&mut r, s.pid, args.seconds);
                }
            }
        }
        if devices::find_cables(&devs).iter().any(|c| c.speaker.is_some() && c.mic.is_some()) {
            let answer = ask("Проверить виртуальный микрофон? Enter — да, 0 и Enter — пропустить:");
            if answer.is_some_and(|a| a != "0") {
                run_loop_test(&mut r, &devs);
            }
        }
    }

    let path = out_dir().join("probe-report.txt");
    r.line("");
    match r.append_to(&path) {
        Ok(()) => println!("Отчёт дописан в файл: {}", path.display()),
        Err(e) => println!("Не удалось записать отчёт {}: {e}", path.display()),
    }
    if !args.no_pause {
        ask("Готово. Нажмите Enter, чтобы закрыть окно.");
    }
}
