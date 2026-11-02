// SPDX-License-Identifier: AGPL-3.0-or-later
// Едва заметный «пятачок» по центру поля. Цвет и прозрачность — из темы (currentColor).
import pignose from '../../../../assets/pignose.svg?raw';

export default function Watermark() {
  return <div className="watermark" aria-hidden="true" dangerouslySetInnerHTML={{ __html: pignose }} />;
}
