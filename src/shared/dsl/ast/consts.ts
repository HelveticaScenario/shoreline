import ts from 'typescript';

const found = new WeakMap<
    ts.SourceFile,
    ReadonlyMap<string, ts.Expression | null>
>();

/**
 * The initializer of every `const` binding in the file, at any depth, by
 * name: null for a name bound by more than one `const`, whose value depends
 * on which scope a reference sits in. Computed once per parsed file.
 */
export function constBindings(
    file: ts.SourceFile,
): ReadonlyMap<string, ts.Expression | null> {
    const known = found.get(file);
    if (known) {
        return known;
    }
    const bindings = new Map<string, ts.Expression | null>();
    const visit = (node: ts.Node) => {
        if (
            ts.isVariableDeclaration(node) &&
            ts.isIdentifier(node.name) &&
            node.initializer &&
            ts.isVariableDeclarationList(node.parent) &&
            // `await using` sets the Const bit too.
            (node.parent.flags & ts.NodeFlags.Const) !== 0 &&
            (node.parent.flags & ts.NodeFlags.Using) === 0
        ) {
            const name = node.name.text;
            bindings.set(name, bindings.has(name) ? null : node.initializer);
        }
        ts.forEachChild(node, visit);
    };
    visit(file);
    found.set(file, bindings);
    return bindings;
}
