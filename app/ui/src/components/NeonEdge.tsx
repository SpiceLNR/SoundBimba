// SPDX-License-Identifier: AGPL-3.0-or-later
// Неоновый провод (тема «Нео-нуар»): обычная плавная кривая плюс горсть искр у одного из
// концов — у выхода или у входа (выбирается по id провода, чтобы у разных проводов по-разному).
// Искры видны только в момент, когда провод моргает: их анимация и моргание задаются в
// theme.css темы одним ритмом. Цвет и свечение искр — тоже там.
import { BaseEdge, getBezierPath, type EdgeProps } from '@xyflow/react';
import { memo, type CSSProperties } from 'react';

/** Разлёт искр: углы (в градусах от направления «прочь от узла») и дальность, px. */
const SPARKS: [number, number][] = [
  [-62, 18],
  [-35, 26],
  [-12, 21],
  [8, 30],
  [30, 23],
  [55, 27],
  [78, 16],
];

function hash(id: string): number {
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) | 0;
  return Math.abs(h);
}

/** Куда летят искры: с выхода (справа у узла) — вправо, со входа (слева у узла) — влево. */
export function sparkOffsets(fromSource: boolean): { dx: number; dy: number }[] {
  const away = fromSource ? 0 : 180;
  return SPARKS.map(([a, r]) => {
    const rad = ((away + a) * Math.PI) / 180;
    return { dx: Math.round(Math.cos(rad) * r), dy: Math.round(Math.sin(rad) * r) };
  });
}

function NeonEdge({ id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, style, markerEnd, interactionWidth }: EdgeProps) {
  const [path] = getBezierPath({ sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition });
  const fromSource = hash(id) % 2 === 0;
  const [x, y] = fromSource ? [sourceX, sourceY] : [targetX, targetY];
  return (
    <>
      <BaseEdge id={id} path={path} style={style} markerEnd={markerEnd} interactionWidth={interactionWidth} />
      <g className="wire-sparks" transform={`translate(${x.toFixed(1)},${y.toFixed(1)})`} aria-hidden="true">
        {sparkOffsets(fromSource).map(({ dx, dy }, i) => (
          <circle key={i} className="wire-spark" r={i % 3 === 0 ? 2.2 : 1.5} style={{ '--dx': `${dx}px`, '--dy': `${dy}px` } as CSSProperties} />
        ))}
      </g>
    </>
  );
}

export default memo(NeonEdge);
