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

/**
 * The first node under `node`, in source order and each before its
 * children, that `test` accepts. With `at`, only nodes whose range
 * (leading trivia included) holds that offset are searched.
 */
export function findDescendant<T extends ts.Node>(
    node: ts.Node,
    test: (candidate: ts.Node) => candidate is T,
    at?: number,
): T | undefined {
    return ts.forEachChild(node, (child): T | undefined => {
        if (at !== undefined && (at < child.pos || at > child.end)) {
            return undefined;
        }
        return test(child) ? child : findDescendant(child, test, at);
    });
}
