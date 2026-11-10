/**
 * Resolving a name to the `const` it refers to, by lexical scope: the
 * nearest enclosing block, function, loop or catch clause that binds the
 * name decides. A `let`, `var`, parameter, loop or catch variable, function
 * or class of that name hides any outer `const`. A `var` binds in its
 * enclosing function, class static block or the file, wherever in it it is
 * declared.
 *
 * Every binding is read as visible throughout its scope, ahead of its
 * declaration too.
 */
import ts from 'typescript';

/** A scope's bindings: a const's initializer, or null for any other kind. */
type Bindings = Map<string, ts.Expression | null>;

const scopes = new WeakMap<ts.Node, Bindings>();

function bindNames(name: ts.BindingName, out: Bindings): void {
    if (ts.isIdentifier(name)) {
        out.set(name.text, null);
        return;
    }
    for (const element of name.elements) {
        if (!ts.isOmittedExpression(element)) {
            bindNames(element.name, out);
        }
    }
}

function isConstList(list: ts.VariableDeclarationList): boolean {
    // `await using` sets the Const bit too.
    return (
        (list.flags & ts.NodeFlags.Const) !== 0 &&
        (list.flags & ts.NodeFlags.Using) === 0
    );
}

function bindList(list: ts.VariableDeclarationList, out: Bindings): void {
    const isConst = isConstList(list);
    for (const decl of list.declarations) {
        if (isConst && ts.isIdentifier(decl.name) && decl.initializer) {
            out.set(decl.name.text, decl.initializer);
        } else {
            bindNames(decl.name, out);
        }
    }
}

/** The names `var` declares anywhere in `node` outside nested functions and
 *  class static blocks. */
function bindHoistedVars(node: ts.Node, out: Bindings): void {
    ts.forEachChild(node, (child) => {
        if (
            ts.isFunctionLike(child) ||
            ts.isClassStaticBlockDeclaration(child)
        ) {
            return;
        }
        if (
            ts.isVariableDeclarationList(child) &&
            (child.flags & ts.NodeFlags.BlockScoped) === 0
        ) {
            for (const decl of child.declarations) {
                bindNames(decl.name, out);
            }
        }
        bindHoistedVars(child, out);
    });
}

/** What `node` binds as a scope, or null when it opens no scope. */
function bindingsOf(node: ts.Node): Bindings | null {
    const known = scopes.get(node);
    if (known) {
        return known;
    }
    const out: Bindings = new Map();
    if (ts.isFunctionLike(node)) {
        if (ts.isFunctionExpression(node) && node.name) {
            out.set(node.name.text, null);
        }
        for (const param of node.parameters) {
            bindNames(param.name, out);
        }
        if ('body' in node && node.body) {
            bindHoistedVars(node.body, out);
        }
    } else if (ts.isClassStaticBlockDeclaration(node)) {
        bindHoistedVars(node.body, out);
    } else if (ts.isClassExpression(node)) {
        if (!node.name) {
            return null;
        }
        out.set(node.name.text, null);
    } else if (
        ts.isForStatement(node) ||
        ts.isForInStatement(node) ||
        ts.isForOfStatement(node)
    ) {
        if (
            node.initializer &&
            ts.isVariableDeclarationList(node.initializer)
        ) {
            bindList(node.initializer, out);
        }
    } else if (ts.isCatchClause(node)) {
        if (node.variableDeclaration) {
            bindNames(node.variableDeclaration.name, out);
        }
    } else if (
        ts.isSourceFile(node) ||
        ts.isBlock(node) ||
        ts.isModuleBlock(node) ||
        ts.isCaseBlock(node)
    ) {
        if (ts.isSourceFile(node)) {
            bindHoistedVars(node, out);
        }
        const statements = ts.isCaseBlock(node)
            ? node.clauses.flatMap((c) => [...c.statements])
            : node.statements;
        for (const statement of statements) {
            if (ts.isVariableStatement(statement)) {
                bindList(statement.declarationList, out);
            } else if (
                (ts.isFunctionDeclaration(statement) ||
                    ts.isClassDeclaration(statement)) &&
                statement.name
            ) {
                out.set(statement.name.text, null);
            }
        }
    } else {
        return null;
    }
    scopes.set(node, out);
    return out;
}

/**
 * The initializer of the `const` the identifier `id` refers to, or null when
 * the nearest binding of its name is not a `const` with an initializer, or
 * nothing binds it. Needs a tree parsed with parent pointers.
 */
export function constInitializerOf(id: ts.Identifier): ts.Expression | null {
    for (let n: ts.Node | undefined = id.parent; n; n = n.parent) {
        const bound = bindingsOf(n)?.get(id.text);
        if (bound !== undefined) {
            return bound;
        }
    }
    return null;
}
