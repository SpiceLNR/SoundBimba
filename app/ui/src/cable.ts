// SPDX-License-Identifier: AGPL-3.0-or-later
// Виртуальный кабель ставит сам человек (мы драйвер не устанавливаем) — здесь только поиск.
import type { AudioDevice } from './api';

/** Официальная страница VB-CABLE. Программа сама туда не ходит — адрес только показываем. */
export const CABLE_URL = 'vb-audio.com/Cable';

/** Название кабеля, если есть обе его стороны: динамик (куда пишем) и микрофон (откуда слышат). */
export function foundCable(devices: AudioDevice[]): string | null {
  const render = devices.find((d) => d.render && d.cable);
  const capture = devices.find((d) => !d.render && d.cable === render?.cable);
  return render && capture ? render.cable : null;
}
