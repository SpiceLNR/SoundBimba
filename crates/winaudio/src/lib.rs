// SPDX-License-Identifier: AGPL-3.0-or-later
//! Обёртки над Windows для SoundBimba: сведения о системе, список звуковых устройств,
//! поиск виртуальных кабелей, потоки устройств (захват и вывод) и поток движка.
//! Системные настройки звука не меняются — кроме `routing` (куда Windows направляет звук отдельной
//! программы; меняет приложение по выбору человека и само возвращает).

pub mod cables;
pub mod devices;
pub mod format;
pub mod latency;
pub mod loopback;
pub mod processes;
pub mod routing;
pub mod runner;
pub mod streams;
pub mod system;
