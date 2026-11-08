// SPDX-License-Identifier: AGPL-3.0-or-later
// Модель карты на стороне интерфейса: та же схема, что в crates/graph (Rust проверяет её при сохранении).
import type { Edge, Node } from '@xyflow/react';
import { signed } from './compressor';
import { voiceOf } from './morpher';
import type { Translate } from './i18n';

export type NodeKind = 'app-source' | 'mic-input' | 'sound-button' | 'volume' | 'compressor' | 'voice-morpher' | 'denoise' | 'speaker-out' | 'virtual-out';
export type Category = 'source' | 'processing' | 'output' | 'button';
export type Params = Record<string, unknown>;

interface KindInfo {
  category: Category;
  input: boolean;
  output: boolean;
  defaults: Params;
}

export const KINDS: Record<NodeKind, KindInfo> = {
  // Порядок здесь — порядок списка «Добавить узел» и цифр 1–9; кнопка — первая (просьба автора
  // 2026-11-08).
  'sound-button': { category: 'button', input: false, output: true, defaults: { file: '' } },
  'app-source': { category: 'source', input: false, output: true, defaults: { exe: '' } },
  'mic-input': { category: 'source', input: false, output: true, defaults: { device: 'default' } },
  volume: { category: 'processing', input: true, output: true, defaults: { gain: 1, mute: false } },
  // Те же значения, что набор «Голос» и COMPRESSOR_DEFAULTS в движке.
  compressor: {
    category: 'processing',
    input: true,
    output: true,
    defaults: { threshold: -24, ratio: 3, attack: 5, release: 120, makeup: 12 },
  },
  // Голос «Обычный»: все эффекты выключены, голос проходит без изменений.
  'voice-morpher': { category: 'processing', input: true, output: true, defaults: { pitch: 0, robot: 0, radio: 0, echo: 0, wobble: 0 } },
  denoise: { category: 'processing', input: true, output: true, defaults: { amount: 1 } },
  'speaker-out': { category: 'output', input: true, output: false, defaults: { device: 'default' } },
  'virtual-out': { category: 'output', input: true, output: false, defaults: { device: 'virtual-1' } },
};

export const KIND_ORDER = Object.keys(KINDS) as NodeKind[];
export const CATEGORY_ORDER: Category[] = ['button', 'source', 'processing', 'output'];

export interface ProjectNode {
  id: string;
  type: NodeKind;
  pos: [number, number];
  params: Params;
}

export interface ProjectEdge {
  from: string;
  to: string;
  kind?: 'audio' | 'control';
}

export interface Project {
  schema: number;
  nodes: ProjectNode[];
  edges: ProjectEdge[];
}

export const SCHEMA = 1;

export interface SoundData extends Record<string, unknown> {
  kind: NodeKind;
  params: Params;
}

export type SoundNode = Node<SoundData, 'sound'>;

export const NODE_WIDTH = 200;
export const NODE_HEIGHT = 64;

export function edgeId(from: string, to: string): string {
  return `${from}->${to}`;
}

export function toFlow(p: Project): { nodes: SoundNode[]; edges: Edge[] } {
  return {
    nodes: p.nodes.map((n) => ({
      id: n.id,
      type: 'sound',
      position: { x: n.pos[0], y: n.pos[1] },
      data: { kind: n.type, params: { ...n.params } },
    })),
    edges: p.edges.map((e) => ({
      id: edgeId(e.from, e.to),
      source: e.from,
      target: e.to,
      data: e.kind && e.kind !== 'audio' ? { kind: e.kind } : undefined,
    })),
  };
}

const round = (v: number) => Math.round(v * 10) / 10;

export function fromFlow(nodes: SoundNode[], edges: Edge[]): Project {
  return {
    schema: SCHEMA,
    nodes: nodes.map((n) => ({
      id: n.id,
      type: n.data.kind,
      pos: [round(n.position.x), round(n.position.y)],
      params: n.data.params,
    })),
    edges: edges.map((e) => {
      const kind = (e.data as { kind?: 'control' } | undefined)?.kind;
      return kind ? { from: e.source, to: e.target, kind } : { from: e.source, to: e.target };
    }),
  };
}

/** Есть ли уже путь to → … → from (тогда провод from → to замкнёт круг). */
export function wouldCreateCycle(edges: Pick<Edge, 'source' | 'target'>[], from: string, to: string): boolean {
  if (from === to) return true;
  const stack = [to];
  const seen = new Set<string>();
  while (stack.length > 0) {
    const id = stack.pop() as string;
    if (id === from) return true;
    if (seen.has(id)) continue;
    seen.add(id);
    for (const e of edges) if (e.source === id) stack.push(e.target);
  }
  return false;
}

export type ConnectCheck = 'ok' | 'cycle' | 'invalid';

export function canConnect(nodes: SoundNode[], edges: Edge[], from: string, to: string): ConnectCheck {
  const a = nodes.find((n) => n.id === from);
  const b = nodes.find((n) => n.id === to);
  if (!a || !b || !KINDS[a.data.kind].output || !KINDS[b.data.kind].input) return 'invalid';
  if (edges.some((e) => e.source === from && e.target === to)) return 'invalid';
  return wouldCreateCycle(edges, from, to) ? 'cycle' : 'ok';
}

export function nextNodeId(nodes: Pick<SoundNode, 'id'>[]): string {
  const max = nodes.reduce((m, n) => Math.max(m, Number(/^n(\d+)$/.exec(n.id)?.[1] ?? 0)), 0);
  return `n${max + 1}`;
}

export function newNode(kind: NodeKind, id: string, x: number, y: number): SoundNode {
  return { id, type: 'sound', position: { x, y }, data: { kind, params: { ...KINDS[kind].defaults } } };
}

const COLUMN_GAP = 64;
const ROW_GAP = 26;
/** Кнопки — плотная стопка. */
const BUTTON_GAP = 10;
const COMPONENT_GAP = 28;
const LOOSE_GAP = 28;
/** Высота «коридора» для длинного провода, проходящего через столбец. */
const CORRIDOR = 10;
const BUTTON_WIDTH = 250;
const BUTTON_HEIGHT = 112;

/** Размер узла: настоящий, если карта его уже измерила, иначе — по типу. */
function sizeOf(n: SoundNode): { w: number; h: number } {
  const button = n.data.kind === 'sound-button';
  return {
    w: n.measured?.width ?? (button ? BUTTON_WIDTH : NODE_WIDTH),
    h: n.measured?.height ?? (button ? BUTTON_HEIGHT : NODE_HEIGHT),
  };
}

/** Элемент столбца: узел или «коридор» длинного провода. */
interface Slot {
  id: string;
  node?: SoundNode;
  h: number;
  preds: string[];
  succs: string[];
}

/** Число пересечений проводов между двумя соседними столбцами при данном порядке. */
function crossings(left: string[], right: string[], slots: Map<string, Slot>): number {
  const pos = new Map(right.map((id, i) => [id, i]));
  const pairs: [number, number][] = [];
  left.forEach((id, i) => {
    for (const s of slots.get(id)!.succs) if (pos.has(s)) pairs.push([i, pos.get(s)!]);
  });
  let n = 0;
  for (let a = 0; a < pairs.length; a++)
    for (let b = a + 1; b < pairs.length; b++)
      if ((pairs[a][0] - pairs[b][0]) * (pairs[a][1] - pairs[b][1]) < 0) n++;
  return n;
}

/**
 * Автораскладка «Упорядочить» (послойная, как в схемах-графах):
 *  - столбцы слева направо (просьба автора 2026-11-08): кнопки, источники, обработка (громкость
 *    после громкости — правее), все выходы — в одном, последнем столбце; если кнопок нет,
 *    источники — в первом столбце; схемы с кнопками — первыми;
 *  - каждая отдельная (не связанная проводами с другими) схема — своей полосой, полосы одна под
 *    другой в том порядке, в каком они стояли на карте;
 *  - порядок в столбцах — с наименьшим числом пересечений проводов («центр тяжести» соседей +
 *    перестановки соседних, лучший из найденных); длинный провод учитывается через «коридоры»
 *    в промежуточных столбцах (по высоте они места не занимают);
 *  - высоты — ровными строками, как автор расставляет руками: узел на одной строке с одним
 *    соседом слева, строки плотно сверху вниз; кнопки — отдельной плотной стопкой;
 *  - неподключённые узлы — ниже всего, каждый в столбце своего типа.
 */
export function arrange(nodes: SoundNode[], edges: Edge[]): SoundNode[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const links = edges.filter((e) => byId.has(e.source) && byId.has(e.target) && e.source !== e.target);
  const preds = new Map<string, string[]>(nodes.map((n) => [n.id, []]));
  const succs = new Map<string, string[]>(nodes.map((n) => [n.id, []]));
  for (const e of links) {
    preds.get(e.target)!.push(e.source);
    succs.get(e.source)!.push(e.target);
  }
  const cat = (id: string) => KINDS[byId.get(id)!.data.kind].category;

  // Столбцы (общие для всей карты — чтобы одинаковые типы стояли друг под другом).
  const procRank = new Map<string, number>();
  const sourceCol = nodes.some((n) => cat(n.id) === 'button') ? 1 : 0;
  const rankOf = (id: string, seen = new Set<string>()): number => {
    if (cat(id) === 'button') return 0;
    if (cat(id) === 'source') return sourceCol;
    const known = procRank.get(id);
    if (known !== undefined) return known;
    if (seen.has(id)) return 1; // циклов быть не должно; защита от зависания
    seen.add(id);
    // По настоящим входам: громкость, в которую идут только кнопки, встаёт в столбец источников
    // (она для остальной схемы и есть источник); обработка без входов — сразу за источниками.
    const ins = preds.get(id)!.filter((p) => cat(p) !== 'output');
    const r = ins.length ? 1 + Math.max(...ins.map((p) => rankOf(p, seen))) : sourceCol + 1;
    procRank.set(id, r);
    return r;
  };
  const processing = nodes.filter((n) => cat(n.id) === 'processing');
  const maxProc = Math.max(0, ...processing.map((n) => rankOf(n.id)));
  const last = Math.max(maxProc, sourceCol) + 1;
  const column = (id: string) => (cat(id) === 'output' ? last : rankOf(id));

  // Ширина столбца — по самому широкому узлу в нём.
  const colWidth = new Array<number>(last + 1).fill(NODE_WIDTH);
  for (const n of nodes) colWidth[column(n.id)] = Math.max(colWidth[column(n.id)], sizeOf(n).w);
  const colX: number[] = [];
  colWidth.reduce((x, w, c) => ((colX[c] = x), x + w + COLUMN_GAP), 0);

  const isLinked = (id: string) => preds.get(id)!.length > 0 || succs.get(id)!.length > 0;
  const byY = (a: SoundNode, b: SoundNode) => a.position.y - b.position.y || a.position.x - b.position.x || a.id.localeCompare(b.id);

  // Отдельные схемы (связанные проводами части), в порядке их верха на карте.
  const seen = new Set<string>();
  const components: SoundNode[][] = [];
  for (const start of nodes.filter((n) => isLinked(n.id)).sort(byY)) {
    if (seen.has(start.id)) continue;
    const comp: SoundNode[] = [];
    const stack = [start.id];
    seen.add(start.id);
    while (stack.length) {
      const id = stack.pop()!;
      comp.push(byId.get(id)!);
      for (const x of [...preds.get(id)!, ...succs.get(id)!]) {
        if (!seen.has(x)) {
          seen.add(x);
          stack.push(x);
        }
      }
    }
    components.push(comp.sort(byY));
  }
  const hasButton = (comp: SoundNode[]) => (comp.some((n) => n.data.kind === 'sound-button') ? 0 : 1);
  components.sort((a, b) => hasButton(a) - hasButton(b)); // порядок сортировки устойчив

  const pos = new Map<string, { x: number; y: number }>();
  let top = 0;
  for (const comp of components) {
    const bottom = layoutComponent(comp, column, last, succs);
    for (const [id, y] of bottom.ys) pos.set(id, { x: colX[column(id)], y: top + y });
    top += bottom.height + COMPONENT_GAP;
  }

  // Неподключённые — ниже всего, каждый в столбце своего типа.
  const looseTop = components.length ? top - COMPONENT_GAP + LOOSE_GAP : 0;
  const looseY = new Array<number>(last + 1).fill(looseTop);
  for (const n of nodes.filter((x) => !isLinked(x.id)).sort(byY)) {
    const c = column(n.id);
    pos.set(n.id, { x: colX[c], y: looseY[c] });
    looseY[c] += sizeOf(n).h + ROW_GAP;
  }

  return nodes.map((n) => ({ ...n, position: pos.get(n.id)! }));
}

/** Кнопки идут раньше всех в своём столбце. */
const buttonRank = (n?: SoundNode) => (n?.data.kind === 'sound-button' ? 0 : 1);

/** Раскладка одной связанной схемы: высота каждого узла (от верха схемы) и общая высота. */
function layoutComponent(
  comp: SoundNode[],
  column: (id: string) => number,
  last: number,
  succs: Map<string, string[]>,
): { ys: Map<string, number>; height: number } {
  // Слоты: узлы и коридоры длинных проводов.
  const slots = new Map<string, Slot>();
  const cols: string[][] = Array.from({ length: last + 1 }, () => []);
  for (const n of comp) {
    slots.set(n.id, { id: n.id, node: n, h: sizeOf(n).h, preds: [], succs: [] });
    cols[column(n.id)].push(n.id); // comp уже в порядке «сверху вниз» на карте
  }
  const link = (a: string, b: string) => {
    slots.get(a)!.succs.push(b);
    slots.get(b)!.preds.push(a);
  };
  for (const n of comp) {
    for (const t of succs.get(n.id)!) {
      let from = n.id;
      for (let c = column(n.id) + 1; c < column(t); c++) {
        const id = `${n.id}->${t}#${c}`;
        slots.set(id, { id, h: CORRIDOR, preds: [], succs: [] });
        cols[c].push(id);
        link(from, id);
        from = id;
      }
      link(from, t);
    }
  }
  const group = (id: string) => buttonRank(slots.get(id)!.node);
  for (const col of cols) col.sort((a, b) => group(a) - group(b)); // кнопки — сверху

  // Порядок в столбцах: центр тяжести соседей туда-обратно + перестановки соседних.
  const index = new Map<string, number>();
  const reindex = () => cols.forEach((c) => c.forEach((id, i) => index.set(id, i)));
  const total = () => cols.slice(0, -1).reduce((sum, c, i) => sum + crossings(c, cols[i + 1], slots), 0);
  const sortBy = (col: string[], side: 'preds' | 'succs') => {
    const centre = (id: string) => {
      const ns = slots.get(id)![side];
      return ns.length ? ns.reduce((s, x) => s + index.get(x)!, 0) / ns.length : index.get(id)!;
    };
    const keyed = col.map((id) => ({ id, key: centre(id) }));
    keyed.sort((a, b) => group(a.id) - group(b.id) || a.key - b.key || index.get(a.id)! - index.get(b.id)!);
    col.splice(0, col.length, ...keyed.map((k) => k.id));
    reindex();
  };
  const transpose = () => {
    let improved = true;
    for (let guard = 0; improved && guard < 20; guard++) {
      improved = false;
      for (let c = 0; c <= last; c++) {
        for (let i = 0; i + 1 < cols[c].length; i++) {
          if (group(cols[c][i]) !== group(cols[c][i + 1])) continue; // кнопки не опускаем под источники
          const local = () =>
            (c > 0 ? crossings(cols[c - 1], cols[c], slots) : 0) + (c < last ? crossings(cols[c], cols[c + 1], slots) : 0);
          const before = local();
          [cols[c][i], cols[c][i + 1]] = [cols[c][i + 1], cols[c][i]];
          if (local() < before) improved = true;
          else [cols[c][i], cols[c][i + 1]] = [cols[c][i + 1], cols[c][i]];
        }
      }
    }
    reindex();
  };
  reindex();
  let best = cols.map((c) => [...c]);
  let bestCount = total();
  for (let pass = 0; pass < 8 && bestCount > 0; pass++) {
    for (let c = 1; c <= last; c++) sortBy(cols[c], 'preds');
    for (let c = last - 1; c >= 0; c--) sortBy(cols[c], 'succs');
    transpose();
    const count = total();
    if (count < bestCount) {
      bestCount = count;
      best = cols.map((c) => [...c]);
    }
  }
  best.forEach((c, i) => cols[i].splice(0, cols[i].length, ...c));

  // Узел, в который идут только кнопки (обычно их громкость), — в середину своего столбца: к нему
  // сходятся провода от всей стопки кнопок, и схема получается симметричной.
  const buttonOnly = (id: string) => {
    const s = slots.get(id)!;
    return !!s.node && s.node.data.kind !== 'sound-button' && s.preds.length > 0 && s.preds.every((p) => slots.get(p)!.node?.data.kind === 'sound-button');
  };
  for (const col of cols) {
    const fed = col.filter(buttonOnly);
    if (!fed.length) continue;
    const rest = col.filter((id) => !buttonOnly(id));
    const realBefore = Math.floor(rest.filter((id) => slots.get(id)!.node).length / 2);
    let at = 0;
    for (let seen = 0; at < rest.length && seen < realBefore; at++) if (slots.get(rest[at])!.node) seen++;
    col.splice(0, col.length, ...rest.slice(0, at), ...fed, ...rest.slice(at));
  }

  // Высоты. Коридоры нужны только для порядка: по высоте место не занимают. Кнопки — отдельная
  // плотная стопка слева и остальную схему не тянут (иначе всё растягивается по высоте кнопок).
  const isButton = (id: string) => slots.get(id)!.node?.data.kind === 'sound-button';
  const rows = cols.map((col) => col.filter((id) => slots.get(id)!.node && !isButton(id)));
  const h = (id: string) => slots.get(id)!.h;
  const y = new Map<string, number>();
  // Настоящие соседи (сквозь коридоры), без кнопок; «близкие» — в соседнем столбце.
  const neighbours = (id: string, side: 'preds' | 'succs') => {
    const out: { id: string; near: boolean }[] = [];
    for (let n of slots.get(id)![side]) {
      let near = true;
      while (!slots.get(n)!.node) {
        n = slots.get(n)![side][0];
        near = false;
      }
      if (!isButton(n)) out.push({ id: n, near });
    }
    return out;
  };

  // Ровные строки (как автор расставляет руками): каждый узел встаёт на одну строку с одним
  // соседом слева — средним из входов (при чётном числе — верхним из двух средних), если тот
  // ещё свободен и это не перекрещивает другие строки. Так получаются цепочки-«строки».
  const rowIndex = new Map<string, number>();
  rows.forEach((col) => col.forEach((id, i) => rowIndex.set(id, i)));
  const root = new Map<string, string>(comp.map((n) => [n.id, n.id]));
  const taken = new Set<string>();
  for (let c = 1; c <= last; c++) {
    let lastUsed = -1;
    for (const v of rows[c]) {
      const ps = neighbours(v, 'preds')
        .filter((n) => n.near)
        .map((n) => n.id)
        .sort((a, b) => rowIndex.get(a)! - rowIndex.get(b)!);
      if (!ps.length) continue;
      const choices = [ps[Math.ceil(ps.length / 2) - 1], ps[Math.floor(ps.length / 2)]];
      const u = choices.find((p) => !taken.has(p) && rowIndex.get(p)! > lastUsed);
      if (!u) continue;
      root.set(v, root.get(u)!);
      taken.add(u);
      lastUsed = rowIndex.get(u)!;
    }
  }
  // Строки укладываются плотно сверху вниз: в каждом столбце узел не выше, чем позволяет узел
  // над ним. Положение строки — по середине её узлов (узлы бывают разной высоты).
  const middle = new Map<string, number>();
  for (const col of rows) for (const id of col) middle.set(root.get(id)!, Math.max(middle.get(root.get(id)!) ?? 0, h(id) / 2));
  for (let changed = true, guard = 0; changed && guard < 500; guard++) {
    changed = false;
    for (const col of rows) {
      for (let i = 1; i < col.length; i++) {
        const u = col[i - 1];
        const v = col[i];
        const need = middle.get(root.get(u)!)! + h(u) / 2 + ROW_GAP + h(v) / 2;
        if (middle.get(root.get(v)!)! < need - 0.01) {
          middle.set(root.get(v)!, need);
          changed = true;
        }
      }
    }
  }
  for (const col of rows) for (const id of col) y.set(id, middle.get(root.get(id)!)! - h(id) / 2);
  const centre = (id: string) => y.get(id)! + h(id) / 2;

  // Кнопки — плотная стопка, отцентрованная напротив того, куда они идут (обычно их громкости):
  // схема оказывается посередине стопки. Несколько групп кнопок — каждая напротив своей цели,
  // без наложений (то же «ближайший неубывающий ряд», что и выше).
  for (const col of cols) {
    const stack = col.filter((id) => slots.get(id)!.node && isButton(id));
    if (!stack.length) continue;
    const want = stack.map((id) => {
      const cs = neighbours(id, 'succs')
        .map((n) => centre(n.id))
        .sort((a, b) => a - b);
      return cs.length ? cs[Math.ceil(cs.length / 2) - 1] - h(id) / 2 : 0;
    });
    const offset: number[] = [];
    stack.reduce((acc, id, i) => ((offset[i] = acc), acc + h(id) + BUTTON_GAP), 0);
    const groups: { sum: number; n: number }[] = [];
    for (let i = 0; i < stack.length; i++) {
      groups.push({ sum: want[i] - offset[i], n: 1 });
      while (groups.length > 1) {
        const g2 = groups[groups.length - 1];
        const g1 = groups[groups.length - 2];
        if (g1.sum / g1.n <= g2.sum / g2.n) break;
        g1.sum += g2.sum;
        g1.n += g2.n;
        groups.pop();
      }
    }
    let i = 0;
    for (const g of groups) for (let k = 0; k < g.n; k++, i++) y.set(stack[i], g.sum / g.n + offset[i]);
  }

  // Схема — от нуля по высоте.
  const real = comp.map((n) => n.id);
  const minY = Math.min(...real.map((id) => y.get(id)!));
  const ys = new Map(real.map((id) => [id, y.get(id)! - minY]));
  const height = Math.max(...real.map((id) => ys.get(id)! + slots.get(id)!.h));
  return { ys, height };
}

/** Короткая подпись под названием узла: что выбрано. */
export function summary(kind: NodeKind, params: Params, t: Translate): string {
  switch (kind) {
    case 'app-source':
      return typeof params.exe === 'string' && params.exe ? params.exe : t('param.exeEmpty');
    case 'sound-button':
      return typeof params.file === 'string' && params.file ? params.file : t('param.fileEmpty');
    case 'volume': {
      const gain = `${Math.round(Number(params.gain ?? 1) * 100)} %`;
      return params.mute === true ? `${gain} · ${t('param.muted')}` : gain;
    }
    case 'compressor':
      return `${signed(Number(params.threshold ?? -24))} ${t('unit.db')} · ${Number(params.ratio ?? 3)}:1 · +${Number(params.makeup ?? 12)} ${t('unit.db')}`;
    case 'denoise':
      return `${t('param.amount')} ${Math.round(Number(params.amount ?? 1) * 100)} %`;
    case 'voice-morpher': {
      const voice = voiceOf(params);
      return voice ? t(`voice.${voice}`) : t('voice.custom');
    }
    case 'mic-input':
    case 'speaker-out':
      return params.device === 'default' || params.device === undefined ? t('param.deviceDefault') : String(params.device);
    case 'virtual-out':
      return '';
  }
}

/** Как узел называется в простом режиме: для программы — имя exe, для кнопки — её подпись. */
export function shortName(n: ProjectNode, t: Translate): string {
  if (n.type === 'app-source' && typeof n.params.exe === 'string' && n.params.exe) return n.params.exe;
  if (n.type === 'sound-button') {
    if (typeof n.params.label === 'string' && n.params.label.trim()) return n.params.label.trim();
    if (typeof n.params.file === 'string' && n.params.file) {
      return (n.params.file.split(/[\\/]/).pop() ?? n.params.file).replace(/\.[^.]+$/, '');
    }
  }
  return t(`node.${n.type}.title`);
}

export interface VolumeRoute {
  id: string;
  /** Что подаёт звук в эту громкость, через запятую. */
  from: string;
  /** Куда звук уходит дальше, через запятую. */
  to: string;
  /** То же списками — простой режим сворачивает длинные перечисления. */
  fromNames: string[];
  toNames: string[];
}

/** Порядок в простом режиме — число simpleOrder в параметрах узла: так он попадает и в файл
 *  проекта, и в шаблон .bmb, а звуковой движок его не читает (и не перезапускается). Узлы без
 *  номера — после пронумерованных, в порядке карты. */
export function simpleSorted<T extends { params: Params }>(items: T[]): T[] {
  const key = (x: T) => (typeof x.params.simpleOrder === 'number' ? x.params.simpleOrder : 1e9);
  return items
    .map((x, i) => ({ x, i }))
    .sort((a, b) => key(a.x) - key(b.x) || a.i - b.i)
    .map((a) => a.x);
}

/** Ставит id на место target (остальные сдвигаются) — новые номера всех элементов раздела. */
export function moveTo(ids: string[], id: string, target: string): Record<string, number> {
  const from = ids.indexOf(id);
  const to = ids.indexOf(target);
  const next = ids.filter((x) => x !== id);
  if (from >= 0 && to >= 0) next.splice(to, 0, id);
  return Object.fromEntries(next.map((x, i) => [x, i]));
}

/** Регуляторы для простого режима (громкость, морфер): «откуда → куда» по соседним узлам. */
export function volumeRoutes(p: Project, t: Translate, kind: NodeKind = 'volume'): VolumeRoute[] {
  const byId = new Map(p.nodes.map((n) => [n.id, n]));
  const names = (ids: string[]) => [...new Set(ids.map((id) => byId.get(id)).filter((n): n is ProjectNode => !!n).map((n) => shortName(n, t)))];
  return simpleSorted(p.nodes.filter((n) => n.type === kind))
    .map((n) => {
      const fromNames = names(p.edges.filter((e) => e.to === n.id).map((e) => e.from));
      const toNames = names(p.edges.filter((e) => e.from === n.id).map((e) => e.to));
      return { id: n.id, from: fromNames.join(', '), to: toNames.join(', '), fromNames, toNames };
    });
}
