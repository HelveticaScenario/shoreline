/** `lines` as a JSDoc comment at `indent`; no comment for no lines. */
export function jsDocComment(lines: string[], indent = ''): string[] {
    if (lines.length === 0) {
        return [];
    }
    return [
        `${indent}/**`,
        ...lines.map((line) =>
            line === '' ? `${indent} *` : `${indent} * ${line}`,
        ),
        `${indent} */`,
    ];
}
