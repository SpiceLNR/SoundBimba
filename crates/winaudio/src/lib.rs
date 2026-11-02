// SPDX-License-Identifier: AGPL-3.0-or-later
//! Обёртки над Windows для SoundBimba: сведения о системе, список звуковых устройств,
//! поиск виртуальных кабелей. Только чтение — ничего не меняет в системе.

pub mod cables;
pub mod devices;
pub mod system;
