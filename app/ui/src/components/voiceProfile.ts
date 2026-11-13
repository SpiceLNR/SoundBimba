// SPDX-License-Identifier: AGPL-3.0-or-later
// Подстройка морфера под голос: высота голоса человека (из настроек) и как её поменять.
import { createContext, useContext } from 'react';

export interface VoiceProfile {
  /** Высота голоса, Гц; null — не подстроено. */
  f0: number | null;
  onChange: (f0: number | null) => void;
}

export const VoiceProfileContext = createContext<VoiceProfile>({ f0: null, onChange: () => {} });

export const useVoiceProfile = () => useContext(VoiceProfileContext);
