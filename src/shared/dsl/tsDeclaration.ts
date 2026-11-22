/**
 * Reads the parts of a TypeScript function declaration that docs show:
 * `$swing(ratio: number, config?: { grid?: number }): GrooveTemplate` gives
 * its name, each param's name, type and optionality, and its return type.
 */

/** A declaration's name, params and return type. */
export interface ParsedDeclaration {
    name: string;
    params: { name: string; type: string; optional: boolean; rest: boolean }[];
    returns: string;
}

const CLOSERS: Record<string, string> = {
    '(': ')',
    '<': '>',
    '[': ']',
    '{': '}',
};

/**
 * The index of each character of `text` that `matches` with no bracket open
 * around it; the `>` of `=>` closes nothing. A closing bracket counts as
 * outside the bracket it closes.
 */
function topLevelIndices(
    text: string,
    matches: (c: string) => boolean,
): number[] {
    const found: number[] = [];
    const open: string[] = [];
    for (let i = 0; i < text.length; i++) {
        const c = text[i];
        if (c === open.at(-1) && !(c === '>' && text[i - 1] === '=')) {
            open.pop();
        } else if (c in CLOSERS) {
            if (open.length === 0 && matches(c)) found.push(i);
            open.push(CLOSERS[c]);
            continue;
        }
        if (open.length === 0 && matches(c)) found.push(i);
    }
    return found;
}

/** `text` split at `separator` wherever no bracket is open, each part trimmed. */
function splitTopLevel(text: string, separator: string): string[] {
    const parts: string[] = [];
    let start = 0;
    for (const i of topLevelIndices(text, (c) => c === separator)) {
        parts.push(text.slice(start, i).trim());
        start = i + 1;
    }
    parts.push(text.slice(start).trim());
    return parts;
}

/** The index just past the bracket that closes the one opening at `open`. */
function closingIndex(text: string, open: number): number {
    const closer = CLOSERS[text[open]];
    const [end] = topLevelIndices(text.slice(open), (c) => c === closer);
    return open + end + 1;
}

export function parseDeclaration(declaration: string): ParsedDeclaration {
    const text = declaration.replace(/\s+/g, ' ').trim();
    let at = text.search(/[<(]/);
    const name = text.slice(0, at);
    const typeParams = new Map<string, string>();
    if (text[at] === '<') {
        const end = closingIndex(text, at);
        for (const param of splitTopLevel(text.slice(at + 1, end - 1), ',')) {
            const [typeName, constraint] = param.split(/ extends /);
            typeParams.set(typeName.trim(), constraint?.trim() ?? 'unknown');
        }
        at = end;
    }
    const end = closingIndex(text, at);
    const paramsText = text.slice(at + 1, end - 1).trim();
    // A type param shows as its constraint, the type a reader passes.
    const resolve = (type: string) =>
        type.replace(/\b[A-Z]\w*\b/g, (word) => typeParams.get(word) ?? word);
    const params = paramsText
        ? splitTopLevel(paramsText, ',').map((param) => {
              const [head, ...type] = splitTopLevel(param, ':');
              const rest = head.startsWith('...');
              const optional = head.endsWith('?');
              return {
                  name: head.replace(/^\.\.\.|\?$/g, ''),
                  optional,
                  rest,
                  type: resolve(type.join(':')),
              };
          })
        : [];
    const returns = resolve(
        text
            .slice(end)
            .replace(/^\s*:|;$/g, '')
            .trim(),
    );
    return { name, params, returns };
}

/** The types joined into one union, each member once. */
export function unionOf(types: string[]): string {
    const distinct = [...new Set(types)];
    if (distinct.length === 1) {
        return distinct[0];
    }
    return [...new Set(distinct.flatMap((t) => splitTopLevel(t, '|')))].join(
        ' | ',
    );
}
