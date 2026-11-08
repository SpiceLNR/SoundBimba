// SPDX-License-Identifier: AGPL-3.0-or-later
// Какая микросхема сейчас открыта: приставка к id узлов («n3/» или «n3/n1/»). Звуковой движок
// знает узлы по полному id (схему ему отдают распакованной), поэтому уровни звука, состояние
// кнопок и нажатия внутри микросхемы идут по полному id.
import { createContext, useContext } from 'react';

export const IdPrefix = createContext('');

export function useFullId(id: string): string {
  return useContext(IdPrefix) + id;
}
