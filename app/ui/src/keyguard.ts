// SPDX-License-Identifier: AGPL-3.0-or-later
// Глушим встроенные сочетания браузерного движка (обновить страницу, поиск, печать, F-клавиши):
// программа — не браузер. Свои горячие клавиши SoundBimba обрабатывает отдельно (hotkeys.ts, только
// внутри окна). В полях ввода оставляем обычное редактирование: копировать, вставить, вырезать,
// выделить, отменить.

const EDITING_KEYS = new Set(['a', 'c', 'v', 'x', 'z', 'y']);

function isEditable(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName));
}

export function installKeyGuard(): void {
  window.addEventListener(
    'keydown',
    (e) => {
      const fKey = /^F\d{1,2}$/.test(e.key);
      const combo = e.ctrlKey || e.metaKey || e.altKey;
      if (fKey || (combo && !(isEditable(e.target) && EDITING_KEYS.has(e.key.toLowerCase())))) {
        // Только отменяем действие браузера; событие идёт дальше — к горячим клавишам SoundBimba.
        e.preventDefault();
      }
    },
    true,
  );
  // Меню браузера по правой кнопке («Обновить», «Исследовать») — только в полях ввода.
  window.addEventListener('contextmenu', (e) => {
    if (!isEditable(e.target)) e.preventDefault();
  });
}
