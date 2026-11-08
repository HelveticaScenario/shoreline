import { describe, expect, test } from 'vitest';
import ts from 'typescript';

import {
    appendArgument,
    appendArrayElement,
    appendObjectArgument,
    appendObjectProperty,
} from '../listEdits';
import type { CodeStyle, ListEdit } from '../listEdits';

const LAYOUT: CodeStyle = {
    bracketSpacing: true,
    printWidth: 80,
    quote: "'",
    tabWidth: 2,
    trailingComma: true,
    useTabs: false,
};

function parse(source: string): ts.SourceFile {
    return ts.createSourceFile(
        'patch.js',
        source,
        ts.ScriptTarget.Latest,
        true,
        ts.ScriptKind.JS,
    );
}

function first<T extends ts.Node>(
    file: ts.SourceFile,
    is: (node: ts.Node) => node is T,
): T {
    let found: T | undefined;
    const visit = (node: ts.Node) => {
        if (!found && is(node)) {
            found = node;
        }
        ts.forEachChild(node, visit);
    };
    visit(file);
    return found!;
}

/** The source after `edit`, with the appended element marked `«…»`. */
function applied(source: string, edit: ListEdit | null, element: string) {
    expect(edit).not.toBeNull();
    const out =
        source.slice(0, edit!.span.start) +
        edit!.newText +
        source.slice(edit!.span.end);
    const at = edit!.span.start + edit!.elementOffset;
    expect(out.slice(at, at + element.length)).toBe(element);
    return out;
}

describe('appendObjectProperty', () => {
    const add = (source: string, prop = 'b: 2') => {
        const file = parse(source);
        const obj = first(file, ts.isObjectLiteralExpression);
        return applied(
            source,
            appendObjectProperty(file, obj, prop, LAYOUT),
            prop,
        );
    };

    test('appends inline to a clean object', () => {
        expect(add(`f({ a: 1 })`)).toBe(`f({ a: 1, b: 2 })`);
        expect(add(`f({})`)).toBe(`f({ b: 2 })`);
    });

    test('repairs stray and missing commas', () => {
        expect(add(`f({ a: 1,, })`)).toBe(`f({ a: 1, b: 2 })`);
        expect(add(`f({ , a: 1 })`)).toBe(`f({  a: 1, b: 2 })`);
        expect(add(`f({ a: 1 c: 3 })`)).toBe(`f({ a: 1, c: 3, b: 2 })`);
        expect(add(`f({,})`)).toBe(`f({ b: 2 })`);
    });

    test('keeps comments while repairing', () => {
        expect(add(`f({ a: 1 /* x, y */,, })`)).toBe(
            `f({ a: 1 /* x, y */, b: 2 })`,
        );
    });

    test('an expanded object gets the property on its own line', () => {
        expect(add(`f({\n  a: 1,,\n})`)).toBe(`f({\n  a: 1,\n  b: 2,\n})`);
    });
});

describe('appendArgument', () => {
    const add = (source: string, text = 'x') => {
        const file = parse(source);
        const call = first(file, ts.isCallExpression);
        return applied(source, appendArgument(file, call, text), text);
    };

    test('appends to the call named, not an outer call of its chain', () => {
        const source = `a.f(1,,).g()`;
        const file = parse(source);
        let inner: ts.CallExpression | undefined;
        const visit = (node: ts.Node) => {
            if (
                ts.isCallExpression(node) &&
                ts.isPropertyAccessExpression(node.expression) &&
                node.expression.name.text === 'f'
            ) {
                inner = node;
            }
            ts.forEachChild(node, visit);
        };
        visit(file);
        expect(applied(source, appendArgument(file, inner!, 'x'), 'x')).toBe(
            `a.f(1, x).g()`,
        );
    });

    test('a trailing comma becomes the separator', () => {
        expect(add(`f(1,)`)).toBe(`f(1, x)`);
        expect(add(`f(,)`)).toBe(`f(x)`);
    });

    test('an expanded argument list gets the argument on its own line', () => {
        expect(add(`f(\n  1,\n)`)).toBe(`f(\n  1,\n  x,\n)`);
    });
});

describe('appendObjectArgument', () => {
    test('hugs the object when it does not fit', () => {
        const source = `f(1)`;
        const file = parse(source);
        const prop = 'a'.repeat(80);
        const out = applied(
            source,
            appendObjectArgument(
                file,
                first(file, ts.isCallExpression),
                prop,
                LAYOUT,
            ),
            prop,
        );
        expect(out).toBe(`f(1, {\n  ${prop},\n})`);
    });
});

describe('appendArrayElement', () => {
    const add = (source: string) => {
        const file = parse(source);
        return applied(
            source,
            appendArrayElement(
                file,
                first(file, ts.isArrayLiteralExpression),
                'x',
            ),
            'x',
        );
    };

    test('keeps array holes, which are elements', () => {
        expect(add(`[1, , 2]`)).toBe(`[1, , 2, x]`);
    });

    test('repairs a missing comma', () => {
        expect(add(`[1 2]`)).toBe(`[1, 2, x]`);
    });
});
