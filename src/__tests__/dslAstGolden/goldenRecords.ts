/**
 * What the golden harness records for one patch source: the output of every
 * static analysis the app runs on patch code, as deterministic JSON.
 *
 * Anchors and variants are found with the `typescript` parser directly, so
 * the harness does not share helpers with the code it checks.
 */
import ts from 'typescript';
import schemas from '@modular/core/schemas.json';
import type { ModuleSchema } from '@modular/core';

import { analyzeSourceSpans } from '../../main/dsl/analyzeSource';
import { executePatchScript } from '../../main/dsl/executor';
import { DOC_EXECUTION_OPTIONS } from '../../main/dsl/__tests__/docExamples';
import { findControlCalls } from '../../shared/dsl/controlCalls';
import {
    extractControls,
    groupCollapseEdit,
} from '../../renderer/dsl/extractControls';
import { computeControlQuickFixes } from '../../renderer/dsl/controlQuickFix';
import {
    computeOutNumericOptionEdit,
    computeOutOptionEdit,
    computeSetOutputGainEdit,
} from '../../renderer/dsl/outSourceEdit';
import {
    isPerChannelLiteral,
    locateSections,
} from '../../renderer/eq/eqSourceLocate';
import type { EqListTarget } from '../../renderer/eq/eqSourceLocate';
import { computeEqSourceEdit } from '../../renderer/eq/eqSourceEdit';
import { computeEqChannelEdit } from '../../renderer/eq/eqChannelEdit';
import {
    API,
    LAYOUT,
} from '../../renderer/dsl/__tests__/controlQuickFixHarness';

const CONTROL_CALLEES = new Set(['$slider', '$btn', '$toggleBtn', '$cGroup']);

function parse(source: string, parents = true): ts.SourceFile {
    return ts.createSourceFile(
        'patch.js',
        source,
        ts.ScriptTarget.Latest,
        parents,
        ts.ScriptKind.JS,
    );
}

function calls(sf: ts.SourceFile): ts.CallExpression[] {
    const out: ts.CallExpression[] = [];
    const visit = (node: ts.Node) => {
        if (ts.isCallExpression(node)) {
            out.push(node);
        }
        ts.forEachChild(node, visit);
    };
    visit(sf);
    return out;
}

/** Object keys sorted, so key order never reads as a change. */
function sortKeys(value: unknown): unknown {
    if (Array.isArray(value)) {
        return value.map(sortKeys);
    }
    if (value && typeof value === 'object') {
        return Object.fromEntries(
            Object.entries(value)
                .filter(([, v]) => v !== undefined)
                .sort(([x], [y]) => (x < y ? -1 : x > y ? 1 : 0))
                .map(([k, v]) => [k, sortKeys(v)]),
        );
    }
    return value;
}

/** Compact JSON with sorted keys. */
function stableJson(value: unknown): string {
    return JSON.stringify(sortKeys(value));
}

/** Indented JSON with sorted keys, where any value that fits in
 *  `INLINE_WIDTH` characters stays on one line. */
const INLINE_WIDTH = 100;

function prettyJson(value: unknown, indent = ''): string {
    const flat = JSON.stringify(value);
    if (
        flat === undefined ||
        flat.length + indent.length <= INLINE_WIDTH ||
        value === null ||
        typeof value !== 'object'
    ) {
        return flat ?? 'null';
    }
    const inner = `${indent}  `;
    if (Array.isArray(value)) {
        return `[\n${value.map((v) => inner + prettyJson(v, inner)).join(',\n')}\n${indent}]`;
    }
    const fields = Object.entries(value).map(
        ([k, v]) => `${inner}${JSON.stringify(k)}: ${prettyJson(v, inner)}`,
    );
    return `{\n${fields.join(',\n')}\n${indent}}`;
}

function errorOf(error: unknown): { error: string } {
    return { error: error instanceof Error ? error.message : String(error) };
}

function guarded<T>(fn: () => T): T | { error: string } {
    try {
        return fn();
    } catch (error) {
        return errorOf(error);
    }
}

function analyzeRecord(source: string) {
    return guarded(() => {
        const result = analyzeSourceSpans(source, schemas as ModuleSchema[]);
        return {
            callSiteSpans: [...result.callSiteSpans].map(([key, span]) => [
                key,
                span.startLine,
                span.endLine,
            ]),
            interpolations: [...result.interpolationResolutions],
            registry: [...result.registry].map(([key, site]) => [
                key,
                site.moduleType,
                [...site.args].map(([arg, span]) => [
                    arg,
                    span.start,
                    span.end,
                ]),
            ]),
        };
    });
}

/** What evaluation reports about the patch's controls and EQ panels, which
 *  rests on the static analysis lining up with V8's call sites. */
function executeRecord(source: string) {
    return guarded(() => {
        const result = executePatchScript(
            source,
            schemas as ModuleSchema[],
            DOC_EXECUTION_OPTIONS,
        );
        // Generated module ids follow creation order, so any unrelated module
        // would shift them; the rest is what the panel shows.
        const withoutId = <T extends { moduleId: string }>({
            moduleId: _moduleId,
            ...rest
        }: T) => rest;
        return {
            buttons: result.buttons.map(withoutId),
            eqPanels: result.eqPanels.length,
            sliders: result.sliders.map(withoutId),
        };
    });
}

function controlCallsRecord(source: string) {
    return guarded(() => {
        // Without parent pointers, as the executor's placement check parses.
        const sf = parse(source, false);
        return findControlCalls(sf).map((c) => ({
            call: [c.call.getStart(sf), c.call.getEnd()],
            group: c.group,
            kind: c.kind,
            name: c.name.getStart(sf),
        }));
    });
}

function controlsRecord(source: string) {
    return guarded(() => {
        const controls = extractControls(source);
        return {
            controls,
            groupCollapse: controls.groups.map((g) => [
                g.callStart,
                groupCollapseEdit(source, g.callStart, true, LAYOUT),
                groupCollapseEdit(source, g.callStart, false, LAYOUT),
            ]),
        };
    });
}

/** Callee-name offsets the EQ panel anchors on: `$eq` and `.eq` of a chain. */
function eqAnchors(sf: ts.SourceFile): number[] {
    return calls(sf).flatMap((c) => {
        const e = c.expression;
        if (ts.isIdentifier(e) && e.text === '$eq') {
            return [e.getStart(sf)];
        }
        if (ts.isPropertyAccessExpression(e) && e.name.text === 'eq') {
            return [e.name.getStart(sf)];
        }
        return [];
    });
}

const quoted = (text: string) => (quote: string) => `${quote}${text}${quote}`;
const raw = (text: string) => () => text;

/** Section edits on `target`'s first and last section. */
function sectionEdits(source: string, anchor: number, target: EqListTarget) {
    const fields = { freq: raw('1000'), type: quoted('peak') };
    const indices = [...new Set([0, Math.max(target.count - 1, 0)])];
    const edit = (op: Parameters<typeof computeEqSourceEdit>[3]) =>
        guarded(() => computeEqSourceEdit(source, anchor, target, op, LAYOUT));
    return {
        add: edit({ fields, kind: 'add' }),
        remove: indices.map((index) => edit({ index, kind: 'remove' })),
        set: indices.map((index) => edit({ fields, index, kind: 'set' })),
        target,
    };
}

function eqRecord(source: string, anchor: number) {
    const found = locateSections(source, anchor);
    if (!found.ok) {
        return { anchor, located: found };
    }
    const { outer } = found;
    const perChannel = outer !== null && isPerChannelLiteral(outer);
    const listLength = (i: number) => {
        const list = outer?.elements[i];
        return list && ts.isArrayLiteralExpression(list)
            ? list.elements.length
            : 0;
    };
    const targets: EqListTarget[] = perChannel
        ? [...new Set([0, outer.elements.length - 1])].map((list) => ({
              count: listLength(list),
              list,
              listCount: outer.elements.length,
          }))
        : [{ count: outer?.elements.length ?? 0, list: null, listCount: 1 }];
    const listCount = targets[0].listCount;
    return {
        anchor,
        channel: {
            addChannel: guarded(() =>
                computeEqChannelEdit(
                    source,
                    anchor,
                    listCount,
                    { from: 0, kind: 'addChannel' },
                    LAYOUT,
                ),
            ),
            removeChannel: guarded(() =>
                computeEqChannelEdit(
                    source,
                    anchor,
                    listCount,
                    { index: listCount - 1, kind: 'removeChannel' },
                    LAYOUT,
                ),
            ),
            split: guarded(() =>
                computeEqChannelEdit(
                    source,
                    anchor,
                    listCount,
                    { kind: 'split' },
                    LAYOUT,
                ),
            ),
        },
        located: {
            ok: true,
            outer: outer?.elements.length ?? null,
            perChannel,
        },
        sections: targets.map((t) => sectionEdits(source, anchor, t)),
    };
}

/** Method-name offsets the VU meter buttons anchor on: `.out` / `.outMono`. */
function outAnchors(sf: ts.SourceFile): number[] {
    return calls(sf).flatMap((c) => {
        const e = c.expression;
        return ts.isPropertyAccessExpression(e) &&
            (e.name.text === 'out' || e.name.text === 'outMono')
            ? [e.name.getStart(sf)]
            : [];
    });
}

function outRecord(source: string, anchor: number) {
    return {
        anchor,
        muteOff: guarded(() =>
            computeOutOptionEdit(source, anchor, 'mute', false),
        ),
        muteOn: guarded(() =>
            computeOutOptionEdit(source, anchor, 'mute', true),
        ),
        panClear: guarded(() =>
            computeOutNumericOptionEdit(source, anchor, 'pan', null),
        ),
        panSet: guarded(() =>
            computeOutNumericOptionEdit(source, anchor, 'pan', 0.25),
        ),
        gainClear: guarded(() =>
            computeOutNumericOptionEdit(source, anchor, 'gain', null),
        ),
        gainSet: guarded(() =>
            computeOutNumericOptionEdit(source, anchor, 'gain', 0.5),
        ),
        soloOff: guarded(() =>
            computeOutOptionEdit(source, anchor, 'solo', false),
        ),
        soloOn: guarded(() =>
            computeOutOptionEdit(source, anchor, 'solo', true),
        ),
    };
}

/** Every token boundary of `source`, trivia start included. */
function tokenOffsets(source: string): number[] {
    const scanner = ts.createScanner(
        ts.ScriptTarget.Latest,
        false,
        ts.LanguageVariant.Standard,
        source,
    );
    const offsets = new Set([0, source.length]);
    for (
        let kind = scanner.scan();
        kind !== ts.SyntaxKind.EndOfFileToken;
        kind = scanner.scan()
    ) {
        offsets.add(scanner.getTokenFullStart());
        offsets.add(scanner.getTokenStart());
        offsets.add(scanner.getTokenEnd());
    }
    return [...offsets].sort((a, b) => a - b);
}

/**
 * Quick fixes at every token boundary. Each distinct, non-empty fix list
 * appears once in `fixes`; `at` holds one `first-last index` line per run of
 * offsets offering the same list.
 */
function quickFixRecord(source: string) {
    const fixes: unknown[] = [];
    const ids = new Map<string, number>();
    const runs: [number, number, number][] = [];
    for (const offset of tokenOffsets(source)) {
        const result = guarded(() =>
            computeControlQuickFixes(source, offset, API, LAYOUT),
        );
        const json = stableJson(result);
        let id = json === '[]' ? -1 : ids.get(json);
        if (id === undefined) {
            id = fixes.push(result) - 1;
            ids.set(json, id);
        }
        const last = runs.at(-1);
        if (last && last[2] === id) {
            last[1] = offset;
        } else {
            runs.push([offset, offset, id]);
        }
    }
    return {
        at: runs
            .filter(([, , id]) => id !== -1)
            .map(([first, last, id]) => `${first}-${last} ${id}`),
        fixes,
    };
}

function sourceRecord(source: string, withQuickFixes: boolean) {
    const sf = parse(source);
    return {
        analyze: analyzeRecord(source),
        controlCalls: controlCallsRecord(source),
        controls: controlsRecord(source),
        eq: eqAnchors(sf).map((a) => eqRecord(source, a)),
        out: outAnchors(sf).map((a) => outRecord(source, a)),
        ...(withQuickFixes
            ? {
                  execute: executeRecord(source),
                  quickFixes: quickFixRecord(source),
              }
            : {}),
        setOutputGain: guarded(() => computeSetOutputGainEdit(source, 0.5)),
    };
}

/**
 * Sources derived from `source` that put the analyzers in the states a
 * patch passes through while it is typed: cut short mid-expression, and
 * each control argument made non-literal.
 */
function variants(source: string): Map<string, string> {
    const out = new Map<string, string>();
    for (const fraction of [0.25, 0.5, 0.75]) {
        out.set(
            `cut${fraction * 100}`,
            source.slice(0, Math.floor(source.length * fraction)),
        );
    }
    const sf = parse(source);
    const splice = (start: number, end: number, text: string) =>
        source.slice(0, start) + text + source.slice(end);
    for (const call of calls(sf)) {
        if (
            !ts.isIdentifier(call.expression) ||
            !CONTROL_CALLEES.has(call.expression.text)
        ) {
            continue;
        }
        call.arguments.forEach((arg, i) => {
            out.set(
                `nonLiteral@${call.getStart(sf)}:${i}`,
                splice(arg.getStart(sf), arg.getEnd(), 'x'),
            );
        });
        const visit = (node: ts.Node) => {
            if (
                node.kind === ts.SyntaxKind.TrueKeyword ||
                node.kind === ts.SyntaxKind.FalseKeyword
            ) {
                out.set(
                    `numericBool@${node.getStart(sf)}`,
                    splice(node.getStart(sf), node.getEnd(), '1'),
                );
            }
            ts.forEachChild(node, visit);
        };
        visit(call);
    }
    return out;
}

/** The full golden record of one corpus source, as snapshot text. */
export function goldenRecord(source: string): string {
    const record = {
        original: sourceRecord(source, true),
        variants: Object.fromEntries(
            [...variants(source)].map(([name, text]) => [
                name,
                sourceRecord(text, false),
            ]),
        ),
    };
    return `${prettyJson(sortKeys(record))}\n`;
}
