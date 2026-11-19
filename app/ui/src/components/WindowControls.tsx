// SPDX-License-Identifier: AGPL-3.0-or-later
// Кнопки окна в стиле темы (2026-11-19): свернуть, развернуть / вернуть размер, закрыть. Системной
// рамки у основного окна нет (`decorations: false`).
//
// Перетаскивание за пустое место верхней панели. Главный путь — `app-region: drag` (app.css): это
// место для Windows — заголовок окна, Windows сама двигает окно мышью и касанием. Если главный
// путь не сработал, нажатие доходит до страницы — запасной путь ниже (`dragWindow`,
// `dragWindowTouch`).
import { PhysicalPosition } from '@tauri-apps/api/dpi';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useState } from 'react';
import type React from 'react';
import { api } from '../api';
import { useT } from '../i18n';

const inTauri = '__TAURI_INTERNALS__' in window;

const ICONS = {
  minimize: 'M6 12.5h12',
  maximize: 'M6.5 6.5h11v11h-11z',
  restore: 'M8.5 9.5h9v8h-9zM6.5 14.5v-8h9',
  close: 'M7 7l10 10M17 7 7 17',
  // «В трей»: стрелка вниз — в лоток (вместо крестика, когда крестик прячет окно в трей).
  tray: 'M12 4.5v9M8 10l4 4 4-4M5.5 14.5v4h13v-4',
} as const;

const fail = (what: string) => (err: unknown) => api.log('error', `window ${what} failed: ${String(err)}`);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Через сколько мс после нажатия мышью решить, что Windows окно не повела. */
const NATIVE_GRACE_MS = 200;
/** На сколько точек сдвинуть указатель, чтобы начать (простое нажатие окно не «отрывает»). */
const MOVE_THRESHOLD = 4;
/** Событие указателя старше передвижки окна + столько мс — ещё про прежнее место окна. */
const SETTLE_MS = 8;

const isDragArea = (t: EventTarget | null): t is HTMLElement => t instanceof HTMLElement && t.closest('[data-drag]') !== null && !t.closest('button, a, input, select, .dropdown');

/**
 * Окно идёт за указателем своими силами. Положение — по сдвигу указателя внутри окна от места,
 * где взялись за панель (а не по экранным координатам страницы: они считаются через положение
 * окна, которое само двигается, — окно дёргалось). Событие, пришедшее раньше, чем окно встало на
 * новое место, пропускаем — иначе сдвиг учли бы дважды. Развёрнутое окно сначала возвращается к
 * обычному размеру; место взятия — на той же доле ширины панели.
 */
function follow(clientX: number, clientY: number) {
  const w = getCurrentWindow();
  let grab = { x: clientX, y: clientY };
  let pos: { x: number; y: number } | null = null;
  let scale = 1;
  let busy = false;
  let movedAt = Infinity;
  let last: { x: number; y: number; t: number } | null = null;
  let started = false;
  const begin = async () => {
    scale = await w.scaleFactor();
    if (await w.isMaximized()) {
      const share = clientX / window.innerWidth;
      await w.unmaximize();
      for (let i = 0; i < 25 && (await w.isMaximized()); i++) await sleep(20);
      await sleep(30);
      grab = { x: share * window.innerWidth, y: clientY };
    }
    const p = await w.outerPosition();
    pos = { x: p.x, y: p.y };
    movedAt = performance.now();
  };
  const step = () => {
    if (busy || !last || !pos || last.t < movedAt + SETTLE_MS) return;
    const dx = (last.x - grab.x) * scale;
    const dy = (last.y - grab.y) * scale;
    last = null;
    if (Math.abs(dx) < 1 && Math.abs(dy) < 1) return;
    const to = { x: Math.round(pos.x + dx), y: Math.round(pos.y + dy) };
    busy = true;
    void w
      .setPosition(new PhysicalPosition(to.x, to.y))
      .then(() => {
        pos = to;
      })
      .catch(fail('move'))
      .finally(() => {
        busy = false;
        movedAt = performance.now();
      });
  };
  return {
    started: () => started,
    move: (x: number, y: number, t: number) => {
      if (!started) {
        if (Math.hypot(x - clientX, y - clientY) < MOVE_THRESHOLD) return;
        started = true;
        void begin().then(step, fail('grab'));
      }
      last = { x, y, t };
      step();
    },
  };
}

/** До какого времени не отвечать на «мышиное» нажатие: после касания браузер шлёт его вдогонку. */
let touchUntil = 0;
let lastPointer = 'mouse';

/**
 * Запасной путь, мышь: просим Windows (`startDragging`); пока Windows ведёт окно, события мыши
 * сюда не приходят. Приходят и после паузы при нажатой кнопке — двигаем окно сами. Двойной щелчок
 * — развернуть / вернуть.
 */
export function dragWindow(e: React.MouseEvent<HTMLElement>): void {
  if (!inTauri || e.button !== 0 || !isDragArea(e.target) || performance.now() < touchUntil) return;
  e.preventDefault();
  const w = getCurrentWindow();
  if (e.detail === 2) {
    void w.toggleMaximize().catch(fail('maximize'));
    return;
  }
  if (e.detail !== 1) return;
  const pressed = performance.now();
  const f = follow(e.clientX, e.clientY);
  const stop = () => {
    window.removeEventListener('mousemove', move);
    window.removeEventListener('mouseup', stop);
    window.removeEventListener('blur', stop);
  };
  const move = (m: MouseEvent) => {
    if ((m.buttons & 1) === 0) return stop();
    if (!f.started() && performance.now() - pressed < NATIVE_GRACE_MS) return;
    const was = f.started();
    f.move(m.clientX, m.clientY, m.timeStamp);
    if (!was && f.started()) api.log('info', 'window drag: Windows did not take it, moving it ourselves');
  };
  window.addEventListener('mousemove', move);
  window.addEventListener('mouseup', stop);
  window.addEventListener('blur', stop);
  void w.startDragging().catch(fail('drag'));
}

/**
 * Запасной путь, касание и перо (так приходит луч контроллера в Virtual Desktop; курсор Windows
 * при этом стоит на месте — замер 2026-11-19): окно ведём сами, по событиям указателя.
 */
export function dragWindowTouch(e: React.PointerEvent<HTMLElement>): void {
  lastPointer = e.pointerType;
  if (!inTauri || e.pointerType === 'mouse' || !e.isPrimary || e.button !== 0 || !isDragArea(e.target)) return;
  touchUntil = performance.now() + 1000;
  api.log('info', `window drag by ${e.pointerType}`);
  e.preventDefault();
  const el = e.currentTarget;
  const id = e.pointerId;
  el.setPointerCapture(id);
  const f = follow(e.clientX, e.clientY);
  const move = (m: PointerEvent) => {
    if (m.pointerId === id) f.move(m.clientX, m.clientY, m.timeStamp);
  };
  const stop = (m: PointerEvent) => {
    if (m.pointerId !== id) return;
    touchUntil = performance.now() + 500;
    el.removeEventListener('pointermove', move);
    el.removeEventListener('pointerup', stop);
    el.removeEventListener('pointercancel', stop);
    el.removeEventListener('lostpointercapture', stop);
    if (el.hasPointerCapture(id)) el.releasePointerCapture(id);
  };
  el.addEventListener('pointermove', move);
  el.addEventListener('pointerup', stop);
  el.addEventListener('pointercancel', stop);
  el.addEventListener('lostpointercapture', stop);
}

/** Двойное касание пустого места панели — развернуть / вернуть (мышь — в `dragWindow`). */
export function maximizeOnDoubleTap(e: React.MouseEvent<HTMLElement>): void {
  if (!inTauri || lastPointer === 'mouse' || !isDragArea(e.target)) return;
  void getCurrentWindow().toggleMaximize().catch(fail('maximize'));
}

function Control({ kind, label, onClick }: { kind: keyof typeof ICONS; label: string; onClick: () => void }) {
  return (
    <button type="button" className={`window-control window-${kind}`} aria-label={label} title={label} onClick={onClick}>
      <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
        <path d={ICONS[kind]} fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </button>
  );
}

/** `toTray` — крестик прячет окно в трей: вместо него оранжевая кнопка «Свернуть в трей». */
export default function WindowControls({ toTray }: { toTray: boolean }) {
  const t = useT();
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    if (!inTauri) return;
    const w = getCurrentWindow();
    const update = () => void w.isMaximized().then(setMaximized);
    update();
    const off = w.onResized(update);
    return () => void off.then((f) => f());
  }, []);
  const w = inTauri ? getCurrentWindow() : null;
  return (
    <div className="window-controls">
      <Control kind="minimize" label={t('window.minimize')} onClick={() => void w?.minimize()} />
      <Control
        kind={maximized ? 'restore' : 'maximize'}
        label={t(maximized ? 'window.restore' : 'window.maximize')}
        onClick={() => void w?.toggleMaximize()}
      />
      {/* Окно закрывается одинаково: прятать ли его в трей, решает Rust (настройка closeToTray). */}
      <Control
        kind={toTray ? 'tray' : 'close'}
        label={t(toTray ? 'window.toTray' : 'window.close')}
        onClick={() => void (w ? w.close() : window.close())}
      />
    </div>
  );
}
