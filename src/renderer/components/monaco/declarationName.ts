import type * as TS from 'typescript';

/**
 * The fully qualified name of the declaration at `position` in `fileName`,
 * as `$mi` or `ModuleChainMi.chorus`, or null where nothing is declared.
 */
export function declarationNameAt(
    ts: typeof TS,
    program: TS.Program,
    fileName: string,
    position: number,
): string | null {
    const sourceFile = program.getSourceFile(fileName);
    if (!sourceFile) {
        return null;
    }
    const nodeAt = (node: TS.Node): TS.Node | undefined =>
        node.getStart(sourceFile) <= position && position < node.end
            ? (ts.forEachChild(node, nodeAt) ?? node)
            : undefined;
    const node = nodeAt(sourceFile);
    const checker = program.getTypeChecker();
    const symbol = node && checker.getSymbolAtLocation(node);
    return symbol
        ? checker.getFullyQualifiedName(symbol).replace(/^global\./, '')
        : null;
}
