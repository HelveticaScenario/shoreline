/**
 * Formats a control quick-fix the way Format Document would: the fix is
 * widened to the smallest statement around it that parses cleanly and sits
 * on its own lines, and that statement is reprinted by Prettier at its
 * indent. A fix with no such statement, or whose statement Prettier rejects,
 * stays as the raw edit computeControlQuickFixes checked.
 */
import * as prettier from 'prettier/standalone';
import * as prettierBabel from 'prettier/plugins/babel';
import * as prettierEstree from 'prettier/plugins/estree';
import ts from 'typescript';

import { parseTransient } from '../../shared/dsl/ast/parse';
import type { PrettierConfig } from '../../shared/ipcTypes';
import { introducesSyntaxErrors, parseErrors } from './syntaxCheck';
import type { ControlQuickFix } from './controlQuickFix';
import { indentAt } from './listEdits';

export async function formatControlQuickFix(
    source: string,
    fix: ControlQuickFix,
    options: PrettierConfig,
): Promise<ControlQuickFix> {
    try {
        return (await formatStatement(source, fix, options)) ?? fix;
    } catch {
        return fix;
    }
}

async function formatStatement(
    source: string,
    fix: ControlQuickFix,
    options: PrettierConfig,
): Promise<ControlQuickFix | null> {
    const edited =
        source.slice(0, fix.span.start) +
        fix.newText +
        source.slice(fix.span.end);
    const controlStart = fix.span.start + fix.controlOffset;
    const controlEnd = fix.span.start + fix.caretOffset;
    const file = parseTransient(edited);
    const unit = formatUnit(file, controlStart, controlEnd, parseErrors(file));
    if (!unit) {
        return null;
    }
    const start = unit.getStart(file);
    const end = unit.getEnd();
    // The statement replaces the whole edit, so it must cover it.
    if (start > fix.span.start || end < fix.span.start + fix.newText.length) {
        return null;
    }
    const indent = indentAt(edited, start);
    const tabs = indent.split('\t').length - 1;
    const indentWidth = indent.length - tabs + tabs * (options.tabWidth ?? 2);

    // The control is found again in Prettier's output as the same
    // occurrence of a call with its callee and label.
    const key = callKey(edited.slice(controlStart, controlEnd));
    if (!key) {
        return null;
    }
    const unitText = edited.slice(start, end);
    const occurrence = matchingCalls(unitText, key).findIndex(
        (c) => c.start === controlStart - start,
    );
    if (occurrence < 0) {
        return null;
    }

    const printed = (
        await prettier.format(unitText, {
            ...options,
            endOfLine: edited.includes('\r\n') ? 'crlf' : 'lf',
            parser: 'babel',
            plugins: [prettierBabel, prettierEstree as never],
            printWidth: Math.max((options.printWidth ?? 80) - indentWidth, 20),
        })
    ).trimEnd();
    const printedControl = matchingCalls(printed, key)[occurrence];
    if (!printedControl) {
        return null;
    }
    const newText = reindent(printed, indent);
    const controlOffset = reindent(
        printed.slice(0, printedControl.start),
        indent,
    ).length;
    const caretOffset = reindent(
        printed.slice(0, printedControl.end),
        indent,
    ).length;

    const delta = fix.newText.length - (fix.span.end - fix.span.start);
    const result: ControlQuickFix = {
        ...fix,
        caretOffset,
        controlOffset,
        newText,
        span: { end: end - delta, start },
    };
    return introducesSyntaxErrors(source, result) ? null : result;
}

/**
 * The innermost statement holding the control that starts and ends its own
 * lines, has no parse errors, and holds no multi-line template literal —
 * reindenting would change that literal's value.
 */
function formatUnit(
    file: ts.SourceFile,
    controlStart: number,
    controlEnd: number,
    errors: readonly ts.Diagnostic[],
): ts.Statement | null {
    const text = file.text;
    let node: ts.Node | undefined = findInnermost(
        file,
        controlStart,
        controlEnd,
    );
    for (; node; node = node.parent) {
        if (!isStatement(node)) {
            continue;
        }
        const start = node.getStart(file);
        const end = node.getEnd();
        const lineBefore = text.slice(
            text.lastIndexOf('\n', start - 1) + 1,
            start,
        );
        const lineAfterEnd = text.indexOf('\n', end);
        const lineAfter = text.slice(
            end,
            lineAfterEnd < 0 ? text.length : lineAfterEnd,
        );
        const clean = !errors.some(
            (d) => d.start !== undefined && d.start >= start && d.start <= end,
        );
        if (
            clean &&
            lineBefore.trim() === '' &&
            /^\s*(?:\/\/.*)?$/.test(lineAfter) &&
            !hasMultilineTemplate(node, file)
        ) {
            return node;
        }
    }
    return null;
}

function findInnermost(
    file: ts.SourceFile,
    start: number,
    end: number,
): ts.Node {
    let node: ts.Node = file;
    for (;;) {
        const child: ts.Node | undefined = node
            .getChildren(file)
            .find((c) => c.getStart(file) <= start && c.getEnd() >= end);
        if (!child || child === node) {
            return node;
        }
        node = child;
    }
}

function isStatement(node: ts.Node): node is ts.Statement {
    return (
        ts.isExpressionStatement(node) ||
        ts.isVariableStatement(node) ||
        ts.isReturnStatement(node) ||
        ts.isFunctionDeclaration(node) ||
        ts.isIfStatement(node) ||
        ts.isForStatement(node) ||
        ts.isForOfStatement(node) ||
        ts.isForInStatement(node) ||
        ts.isWhileStatement(node)
    );
}

function hasMultilineTemplate(node: ts.Node, file: ts.SourceFile): boolean {
    if (
        (ts.isNoSubstitutionTemplateLiteral(node) ||
            ts.isTemplateExpression(node)) &&
        node.getText(file).includes('\n')
    ) {
        return true;
    }
    return node.getChildren(file).some((c) => hasMultilineTemplate(c, file));
}

function reindent(text: string, indent: string): string {
    return text
        .split('\n')
        .map((line, i) =>
            i === 0 || line === '' || line === '\r' ? line : indent + line,
        )
        .join('\n');
}

/** `callee(firstArg` of the control call `text`, e.g. `$slider('fm'`. */
function callKey(text: string): string | null {
    const file = parseTransient(text);
    const stmt = file.statements[0];
    if (
        !stmt ||
        !ts.isExpressionStatement(stmt) ||
        !ts.isCallExpression(stmt.expression)
    ) {
        return null;
    }
    return keyOf(stmt.expression, file);
}

function keyOf(call: ts.CallExpression, file: ts.SourceFile): string {
    const first = call.arguments[0];
    return `${call.expression.getText(file)}(${first ? first.getText(file) : ''}`;
}

/** Calls in `text` whose key is `key`, in source order. */
function matchingCalls(
    text: string,
    key: string,
): { start: number; end: number }[] {
    const file = parseTransient(text);
    const found: { start: number; end: number }[] = [];
    const visit = (node: ts.Node) => {
        if (ts.isCallExpression(node) && keyOf(node, file) === key) {
            found.push({ end: node.getEnd(), start: node.getStart(file) });
        }
        ts.forEachChild(node, visit);
    };
    visit(file);
    return found;
}
