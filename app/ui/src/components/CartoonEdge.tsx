// SPDX-License-Identifier: AGPL-3.0-or-later
// Мультяшный провод (детские темы): толстая кривая с тёмной обводкой, по ней бегут огоньки.
// Обводка — отдельный путь под проводом, а не CSS-тень (filter: drop-shadow): тени вокруг
// бегущих чёрточек пересчитывались каждый кадр, и окно с большой схемой почти вставало
// (замер 2026-11-19: детская тема ≈0,6 ядра в покое против ≈0,1 у «Бимбы»).
import { BaseEdge, getBezierPath, type EdgeProps } from '@xyflow/react';
import { memo } from 'react';

function CartoonEdge({ id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, style, markerEnd, interactionWidth }: EdgeProps) {
  const [path] = getBezierPath({ sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition });
  return (
    <>
      <path className="wire-outline" d={path} fill="none" aria-hidden="true" />
      <BaseEdge id={id} path={path} style={style} markerEnd={markerEnd} interactionWidth={interactionWidth} />
    </>
  );
}

export default memo(CartoonEdge);
