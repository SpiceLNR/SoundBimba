// SPDX-License-Identifier: AGPL-3.0-or-later
// Перестановка элементов простого режима: тянуть за ручку мышью (или стрелками с клавиатуры).
// Сделано на событиях указателя, а не на системном перетаскивании: системное в окне Tauri
// перехватывается (там оно для перетаскивания файлов).
import { useState, type KeyboardEvent, type PointerEvent } from 'react';
import { moveTo } from '../graph';

export interface Reorder {
  /** Свойства ручки элемента. */
  handle: (id: string) => {
    onPointerDown: (e: PointerEvent<HTMLElement>) => void;
    onPointerMove: (e: PointerEvent<HTMLElement>) => void;
    onPointerUp: (e: PointerEvent<HTMLElement>) => void;
    onPointerCancel: () => void;
    onKeyDown: (e: KeyboardEvent<HTMLElement>) => void;
  };
  /** Свойства самого элемента (куда можно бросить). */
  item: (id: string) => { 'data-reorder': string; 'data-id': string; className: string };
}

/** ids — элементы раздела в нынешнем порядке; onMove получает новые номера всех элементов. */
export function useReorder(group: string, ids: string[], onMove: (order: Record<string, number>) => void): Reorder {
  const [drag, setDrag] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const under = (x: number, y: number) =>
    (document.elementFromPoint(x, y)?.closest(`[data-reorder="${group}"]`) as HTMLElement | null)?.dataset.id ?? null;
  const stop = () => {
    setDrag(null);
    setOver(null);
  };
  return {
    handle: (id) => ({
      onPointerDown: (e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        setDrag(id);
        setOver(id);
      },
      onPointerMove: (e) => {
        if (drag === id) setOver(under(e.clientX, e.clientY));
      },
      onPointerUp: (e) => {
        if (drag !== id) return;
        const target = under(e.clientX, e.clientY);
        stop();
        if (target && target !== id) onMove(moveTo(ids, id, target));
      },
      onPointerCancel: stop,
      onKeyDown: (e) => {
        const i = ids.indexOf(id);
        const step = e.key === 'ArrowUp' || e.key === 'ArrowLeft' ? -1 : e.key === 'ArrowDown' || e.key === 'ArrowRight' ? 1 : 0;
        const target = ids[i + step];
        if (!step || !target) return;
        e.preventDefault();
        onMove(moveTo(ids, id, target));
      },
    }),
    item: (id) => ({
      'data-reorder': group,
      'data-id': id,
      className: drag === id ? ' reorder-dragging' : drag && over === id ? ' reorder-over' : '',
    }),
  };
}
