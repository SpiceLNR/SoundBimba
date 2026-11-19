// SPDX-License-Identifier: AGPL-3.0-or-later
// Провод с ползунком (тема «Night Hub»): по проводу от выхода ко входу изредка пробегает
// круглый ползунок, как у видеоплеера. Как бежать — в theme.css темы; сдвиг по id провода, чтобы
// ползунки шли вразнобой. Без CSS-теней: только движение по пути (дёшево для окна, см. CartoonEdge).
import { BaseEdge, getBezierPath, type EdgeProps } from '@xyflow/react';
import { memo, type CSSProperties } from 'react';
import { sparkDelay } from './MagicEdge';

function TimelineEdge({ id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, style, markerEnd, interactionWidth }: EdgeProps) {
  const [path] = getBezierPath({ sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition });
  const delay = { animationDelay: `-${sparkDelay(id)}s` } as CSSProperties;
  return (
    <>
      <BaseEdge id={id} path={path} style={style} markerEnd={markerEnd} interactionWidth={interactionWidth} />
      <circle className="wire-playhead" r={4.5} aria-hidden="true" style={{ ...delay, offsetPath: `path('${path}')` }} />
    </>
  );
}

export default memo(TimelineEdge);
