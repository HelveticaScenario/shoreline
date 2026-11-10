import ts from 'typescript';
import { describe, expect, test } from 'vitest';

import { findControlCalls } from '../../controlCalls';
import { negatableNumber, signedNumericLiteral } from '../literals';
import { dottedName, propertyName } from '../names';
import { parseDsl, parseTransient } from '../parse';

describe('parseDsl', () => {
    test('one source text parses to one shared tree', () => {
        const source = `$sine('c4').out() // ${Math.random()}`;
        expect(parseDsl(source)).toBe(parseDsl(source));
    });

    test('a tree is reparsed once four newer sources have been parsed', () => {
        const first = `$saw('c2') // ${Math.random()}`;
        const tree = parseDsl(first);
        for (let i = 0; i < 4; i++) {
            parseDsl(`${first} ${i}`);
        }
        expect(parseDsl(first)).not.toBe(tree);
    });

    test('a recently used tree outlives newer ones', () => {
        const kept = `$saw('c2') // ${Math.random()}`;
        const tree = parseDsl(kept);
        for (let i = 0; i < 8; i++) {
            parseDsl(kept);
            parseDsl(`${kept} ${i}`);
        }
        expect(parseDsl(kept)).toBe(tree);
    });

    test('a transient parse leaves the memoised tree in place', () => {
        const buffer = `$saw('c2') // ${Math.random()}`;
        const tree = parseDsl(buffer);
        for (let i = 0; i < 8; i++) {
            expect(parseTransient(`${buffer} ${i}`)).not.toBe(
                parseTransient(`${buffer} ${i}`),
            );
        }
        expect(parseDsl(buffer)).toBe(tree);
    });

    test('parses as JavaScript with parent pointers', () => {
        const file = parseDsl('const a = <b>1</b>');
        const call = file.statements[0];
        expect(call.parent).toBe(file);
        expect(file.languageVariant).toBe(ts.LanguageVariant.JSX);
    });
});

describe('findControlCalls', () => {
    test('scans a tree once', () => {
        const file = parseDsl(`$slider('a', 1, 0, 2) // ${Math.random()}`);
        expect(findControlCalls(file)).toBe(findControlCalls(file));
    });
});

function expression(text: string): ts.Expression {
    const statement = parseDsl(`(${text})`).statements[0];
    if (!ts.isExpressionStatement(statement)) {
        throw new Error(text);
    }
    const paren = statement.expression;
    return ts.isParenthesizedExpression(paren) ? paren.expression : paren;
}

describe('signedNumericLiteral', () => {
    test.each([
        ['1.5', null, '1.5'],
        ['-2', '-', '2'],
        ['+0x10', '+', '16'],
    ])('%s', (text, sign, literal) => {
        const found = signedNumericLiteral(expression(text));
        expect(found?.sign).toBe(sign);
        expect(found?.literal.text).toBe(literal);
    });

    test.each(['!1', '~1', '-x', "'1'", '--1'])('%s is not one', (text) => {
        expect(signedNumericLiteral(expression(text))).toBeNull();
    });
});

describe('negatableNumber', () => {
    test.each([
        ['1.5', 1.5],
        ['-2', -2],
        ['1e-7', 1e-7],
        ['-0x10', -16],
        ['+1', null],
        ["'1'", null],
        ['-x', null],
    ])('%s', (text, value) => {
        expect(negatableNumber(expression(text))).toBe(value);
    });
});

describe('names', () => {
    test('dottedName ignores what sits between the dots', () => {
        expect(dottedName(expression('$tr808\n  // voice\n  .kick'))).toBe(
            '$tr808.kick',
        );
        expect(dottedName(expression('f().kick'))).toBeNull();
    });

    test('propertyName reads static keys only', () => {
        const obj = expression(
            "{ a: 1, 'b c': 2, 3: 3, [d]: 4, ...e, f, g() {} }",
        ) as ts.ObjectLiteralExpression;
        expect(obj.properties.map(propertyName)).toEqual([
            'a',
            'b c',
            '3',
            null,
            null,
            'f',
            'g',
        ]);
    });
});
