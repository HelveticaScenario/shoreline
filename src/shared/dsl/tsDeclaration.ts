/**
 * Reads the parts of a TypeScript function declaration that docs show:
 * `$swing(ratio: number, config?: { grid?: number }): GrooveTemplate` gives
 * its name, each param's name, type and optionality, and its return type.
 * Each type is split into the members of its union, so overloads can be
 * merged member by member.
 */
import ts from 'typescript';

export interface DeclaredParam {
    name: string;
    /** The members of the param's type union, one for a plain type. */
    types: string[];
    optional: boolean;
    rest: boolean;
}

export interface ParsedDeclaration {
    name: string;
    params: DeclaredParam[];
    returns: string[];
}

function parse(declaration: string): {
    file: ts.SourceFile;
    fn: ts.FunctionDeclaration & {
        name: ts.Identifier;
        type: ts.TypeNode;
    };
} {
    const file = ts.createSourceFile(
        'declaration.ts',
        `declare function ${declaration};`,
        ts.ScriptTarget.Latest,
        false,
        ts.ScriptKind.TS,
    );
    const [fn] = file.statements;
    if (!fn || !ts.isFunctionDeclaration(fn) || !fn.name || !fn.type) {
        throw new Error(`Not a function declaration: ${declaration}`);
    }
    return {
        file,
        fn: fn as typeof fn & { name: ts.Identifier; type: ts.TypeNode },
    };
}

export function parseDeclaration(declaration: string): ParsedDeclaration {
    const { file, fn } = parse(declaration);
    // A type param shows as its constraint, the type a reader passes.
    const constraints = new Map(
        (fn.typeParameters ?? []).map((p) => [
            p.name.text,
            p.constraint?.getText(file) ?? 'unknown',
        ]),
    );
    const members = (node: ts.TypeNode | undefined): string[] =>
        (node && ts.isUnionTypeNode(node) ? node.types : [node]).map((t) =>
            (t?.getText(file) ?? 'unknown').replace(
                /\b\w+\b/g,
                (word) => constraints.get(word) ?? word,
            ),
        );
    return {
        name: fn.name.text,
        params: fn.parameters.map((p) => ({
            name: p.name.getText(file),
            optional: p.questionToken !== undefined,
            rest: p.dotDotDotToken !== undefined,
            types: members(p.type),
        })),
        returns: members(fn.type),
    };
}

/**
 * `declaration` as a chain method returning `returns`: `$delay(input: X,
 * time: Y): Z` is `delay(time: Y): Plain` for `returns` `Plain`, the receiver
 * standing in for the first param.
 */
export function chainDeclaration(declaration: string, returns: string): string {
    const { file, fn } = parse(declaration);
    const typeParams = fn.typeParameters
        ? `<${fn.typeParameters.map((p) => p.getText(file)).join(', ')}>`
        : '';
    const params = fn.parameters
        .slice(1)
        .map((p) => p.getText(file))
        .join(', ');
    return `${fn.name.text.replace(/^\$/, '')}${typeParams}(${params}): ${returns}`;
}

/** The union of every member of `unions`, each once. */
export function unionOf(unions: string[][]): string {
    return [...new Set(unions.flat())].join(' | ');
}
