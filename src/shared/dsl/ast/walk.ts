import ts from 'typescript';

/** Call `visit` on every node under `node`, each before its children. */
export function forEachDescendant(
    node: ts.Node,
    visit: (descendant: ts.Node) => void,
): void {
    ts.forEachChild(node, (child) => {
        visit(child);
        forEachDescendant(child, visit);
    });
}
