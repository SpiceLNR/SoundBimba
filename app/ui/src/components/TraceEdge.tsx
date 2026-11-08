// SPDX-License-Identifier: AGPL-3.0-or-later
// Провод-дорожка, как на печатной плате (тема «Инженерская»): прямые участки и скосы под 45°.
// Из выхода — вправо, скос, вниз или вверх, скос, вправо к входу: четыре излома. Поворот —
// перед самым входом: длинная дорожка идёт по своей полосе (раскладка оставляет её свободной)
// и не задевает узлы по пути. Вертикальный участок у каждой дорожки чуть сдвинут (по её id),
// чтобы параллельные дорожки не сливались.
// Если вход левее выхода — обходит петлёй с теми же скосами.
import { BaseEdge, type EdgeProps } from '@xyflow/react';
import { memo } from 'react';

/** Размер скоса и отступ от узла. */
const CHAMFER = 14;
const LEAD = 22;
/** Где поворачивать: столько от входа (в промежутке между столбцами). */
const TURN = 34;
/** Насколько сдвигать вертикальные участки соседних дорожек. */
const SPREAD = 10;

function offsetOf(id: string): number {
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) | 0;
  return ((Math.abs(h) % 5) - 2) * SPREAD;
}

/** Путь из точек; между ними — прямые. */
const path = (pts: [number, number][]) => pts.map(([x, y], i) => `${i ? 'L' : 'M'}${x.toFixed(1)},${y.toFixed(1)}`).join(' ');

export function tracePath(sx: number, sy: number, tx: number, ty: number, id: string): string {
  const dy = ty - sy;
  const dir = Math.sign(dy) || 1;
  if (tx - sx >= 2 * LEAD) {
    if (Math.abs(dy) < 1) return path([[sx, sy], [tx, ty]]);
    // Скос не больше половины высоты и ширины, иначе — сразу диагональю.
    const c = Math.min(CHAMFER, Math.abs(dy) / 2, (tx - sx) / 4);
    const mid = Math.min(Math.max(tx - TURN + offsetOf(id), sx + LEAD), tx - LEAD);
    return path([
      [sx, sy],
      [mid - c, sy],
      [mid, sy + dir * c],
      [mid, ty - dir * c],
      [mid + c, ty],
      [tx, ty],
    ]);
  }
  // Вход левее: вправо из выхода, по середине между узлами — влево, и снова вправо ко входу.
  const midY = (sy + ty) / 2 + offsetOf(id);
  const right = sx + LEAD;
  const left = tx - LEAD;
  const c = Math.min(CHAMFER, Math.abs(midY - sy) / 2, Math.abs(ty - midY) / 2);
  const d1 = Math.sign(midY - sy) || 1;
  const d2 = Math.sign(ty - midY) || 1;
  return path([
    [sx, sy],
    [right - c, sy],
    [right, sy + d1 * c],
    [right, midY - d1 * c],
    [right - c, midY],
    [left + c, midY],
    [left, midY + d2 * c],
    [left, ty - d2 * c],
    [left + c, ty],
    [tx, ty],
  ]);
}

function TraceEdge({ id, sourceX, sourceY, targetX, targetY, style, markerEnd, interactionWidth }: EdgeProps) {
  return <BaseEdge id={id} path={tracePath(sourceX, sourceY, targetX, targetY, id)} style={style} markerEnd={markerEnd} interactionWidth={interactionWidth} />;
}

export default memo(TraceEdge);
