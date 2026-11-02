// SPDX-License-Identifier: AGPL-3.0-or-later
// Разрешённые лицензии (CLAUDE.md, правило 3) и разбор SPDX-выражений вида "(MIT OR Apache-2.0) AND Zlib".

export const ALLOWED = new Set([
  'MIT',
  'MIT-0',
  'Apache-2.0',
  'BSD-2-Clause',
  'BSD-3-Clause',
  'MPL-2.0',
  'Zlib',
  'ISC',
  'OFL-1.1',
]);

// Исключения к лицензиям, которые только расширяют права (например, Apache-2.0 WITH LLVM-exception).
const ALLOWED_EXCEPTIONS = new Set(['LLVM-exception']);

function tokenize(expr) {
  return expr.replace(/\(/g, ' ( ').replace(/\)/g, ' ) ').replace(/\//g, ' OR ').trim().split(/\s+/);
}

/** true, если выражение допускает использование под разрешёнными лицензиями. */
export function isAllowed(expr) {
  if (!expr) return false;
  const tokens = tokenize(expr);
  let i = 0;
  const atom = () => {
    const tok = tokens[i++];
    if (tok === '(') {
      const v = orExpr();
      i++; // ')'
      return v;
    }
    let ok = ALLOWED.has(tok);
    if (tokens[i] === 'WITH') {
      ok = ok && ALLOWED_EXCEPTIONS.has(tokens[i + 1]);
      i += 2;
    }
    return ok;
  };
  const andExpr = () => {
    let v = atom();
    while (tokens[i] === 'AND') {
      i++;
      v = atom() && v;
    }
    return v;
  };
  const orExpr = () => {
    let v = andExpr();
    while (tokens[i] === 'OR') {
      i++;
      v = andExpr() || v;
    }
    return v;
  };
  return orExpr();
}

/** Печатает итог и завершает процесс с ошибкой, если есть неразрешённые. */
export function report(kind, items) {
  const bad = items.filter((x) => !isAllowed(x.license));
  console.log(`${kind}: ${items.length} packages checked, ${bad.length} need review`);
  for (const b of bad) console.log(`  REVIEW  ${b.name}@${b.version}  license: ${b.license ?? '(none)'}`);
  if (bad.length) process.exitCode = 1;
}
