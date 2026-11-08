// SPDX-License-Identifier: AGPL-3.0-or-later
// Волшебный провод (тема «Магическая»): светящаяся нить-кривая, по которой изредка пробегает
// искорка от выхода ко входу. Искорка — второй путь поверх провода с короткой чёрточкой;
// её бег задаётся в theme.css темы. Когда бежать — сдвиг по id провода, чтобы искорки
// пробегали не по всем проводам разом.
import { BaseEdge, getBezierPath, type EdgeProps } from '@xyflow/react';
import { memo, type CSSProperties } from 'react';

/** Сдвиг начала бега искорки, секунды (0…19; искорка пробегает раз в 20 с). */
export function sparkDelay(id: string): number {
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) | 0;
  return Math.abs(h) % 20;
}

function MagicEdge({ id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, style, markerEnd, interactionWidth }: EdgeProps) {
  const [path] = getBezierPath({ sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition });
  return (
    <>
      <BaseEdge id={id} path={path} style={style} markerEnd={markerEnd} interactionWidth={interactionWidth} />
      <path
        className="wire-wisp"
        d={path}
        pathLength={100}
        fill="none"
        aria-hidden="true"
        style={{ animationDelay: `-${sparkDelay(id)}s` } as CSSProperties}
      />
    </>
  );
}

export default memo(MagicEdge);
