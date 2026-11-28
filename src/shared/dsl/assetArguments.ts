/**
 * Which kind of file a string literal names, told from the DSL call it is a
 * positional argument of: `$sampler('/…')` takes a WAV file, `$mi.sixOp`'s
 * voice a DX7 voice, `$v.image` an image.
 *
 * Takes the `typescript` namespace as an argument: it runs inside Monaco's
 * TypeScript worker against the worker's own compiler.
 */

import type * as TS from 'typescript';
import type { AssetKind } from '../assetFiles';

/**
 * The file kind each positional argument takes, by index, of every DSL call
 * that takes a file, keyed by dotted call name (`$sampler`, `$v.image`).
 */
export type AssetArguments = Record<string, Record<number, AssetKind>>;

/** The calls that take a file without being a module. */
export const GLOBAL_ASSET_ARGUMENTS: AssetArguments = {
    '$v.image': { 0: 'image' },
    '$v.video': { 0: 'video' },
    $wavMeta: { 0: 'wav' },
};

/**
 * The file kind the string literal holding `position` names, when it is
 * written straight as a positional argument of a call in `assets`, or of the
 * chain form of one of `chainable`, which reads its receiver as argument 0.
 */
export function assetKindAt(
    ts: typeof TS,
    sourceFile: TS.SourceFile,
    assets: AssetArguments,
    chainable: ReadonlySet<string>,
    position: number,
): AssetKind | undefined {
    const literal = literalAt(ts, sourceFile, position);
    const call = literal?.parent;
    if (!call || !ts.isCallExpression(call)) {
        return undefined;
    }
    const index = call.arguments.indexOf(literal);
    if (index === -1) {
        return undefined;
    }

    const names: string[] = [];
    let node: TS.Expression = call.expression;
    while (ts.isPropertyAccessExpression(node)) {
        names.unshift(node.name.text);
        node = node.expression;
    }
    if (ts.isIdentifier(node)) {
        const direct = [node.text, ...names].join('.');
        if (Object.hasOwn(assets, direct)) {
            return assets[direct][index];
        }
    }
    // `sig.mi.sixOp(voice)`: the longest trailing path that names a module.
    for (let start = 0; start < names.length; start++) {
        const name = `$${names.slice(start).join('.')}`;
        if (chainable.has(name) && Object.hasOwn(assets, name)) {
            return assets[name][index + 1];
        }
    }
    return undefined;
}

/** The string literal whose text holds `position`, past its opening quote. */
function literalAt(
    ts: typeof TS,
    sourceFile: TS.SourceFile,
    position: number,
): TS.StringLiteral | TS.NoSubstitutionTemplateLiteral | undefined {
    const visit = (
        node: TS.Node,
    ): TS.StringLiteral | TS.NoSubstitutionTemplateLiteral | undefined => {
        if (position <= node.getStart(sourceFile) || position > node.getEnd()) {
            return undefined;
        }
        if (
            ts.isStringLiteral(node) ||
            ts.isNoSubstitutionTemplateLiteral(node)
        ) {
            return node;
        }
        return ts.forEachChild(node, visit);
    };
    return ts.forEachChild(sourceFile, visit);
}
