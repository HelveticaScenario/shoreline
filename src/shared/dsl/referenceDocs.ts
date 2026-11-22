/**
 * The shape every reference entry's docs take — modules and globals — and
 * the two renderings of it: compact JSDoc for Monaco's hover and completion,
 * and a `ReferenceEntry` the Docs panel lays out in full. Type docs
 * (`TYPE_DOCS`) keep their own shape.
 */
import { formatOutputMeta, paramMeta } from './paramRange';
import type { EnumVariantInfo, Schema } from './schemaTypeResolver';
import { getEnumVariants, schemaToTypeExpr } from './schemaTypeResolver';
import { parseDeclaration, unionOf } from './tsDeclaration';

export interface DocExample {
    /** What you hear, e.g. "Acid bass sweep". */
    title: string;
    /** A complete patch that runs as written. */
    code: string;
}

/** Rows of inline-markdown cells; the first column is each row's key. */
export interface DocTable {
    columns: string[];
    rows: string[][];
}

export interface DocSection {
    title: string;
    /** Inline markdown; empty when the table says it all. */
    body: string;
    table?: DocTable;
    examples: DocExample[];
}

export interface DocContent {
    /** A few words naming the entry, for reference lists. */
    summary: string;
    /** One to three sentences of inline markdown. */
    description: string;
    categories: string[];
    sections: DocSection[];
    examples: DocExample[];
}

export interface ParamDoc {
    name: string;
    /** TypeScript type of the param, when it has a readable one. */
    type?: string;
    description?: string;
    /** Range and default, e.g. `0–5, default 2.5`. */
    meta?: string;
    variants?: EnumVariantInfo[];
    /** Takes every remaining argument, as `...name`. */
    rest?: boolean;
}

/** A value an entry gives: a module's output, or a member of what a DSL
 *  function returns, such as a track tap's `.level`. */
export interface ValueDoc {
    name: string;
    description: string;
    /** Range, e.g. `0–5` or `gate`. */
    meta?: string;
}

export interface OutputDoc extends ValueDoc {
    /** The output a bare reference to the entry reads. */
    main: boolean;
}

/** What a DSL function gives back. Schema modules list outputs instead. */
export interface ReturnDoc {
    /** TypeScript type, every overload's joined. */
    type: string;
    description: string;
    meta?: string;
    /** Members read with a dot, named with it. */
    members?: ValueDoc[];
}

export interface ReferenceEntry {
    name: string;
    /** Call shape with param names only, e.g. `$lpf(input, cutoff, { … })`. */
    signature: string;
    params: ParamDoc[];
    outputs?: OutputDoc[];
    returns?: ReturnDoc;
    docs: DocContent;
    /** The module's sound and params may still change between releases. */
    experimental?: boolean;
}

function moduleSignature(module: Schema): string {
    const positionalKeys = new Set(module.positionalArgs.map((a) => a.name));
    const required: string[] = module.paramsSchema.required ?? [];
    const parts: string[] = module.positionalArgs.map((a) =>
        required.includes(a.name) ? a.name : `${a.name}?`,
    );
    const configKeys = Object.keys(module.paramsSchema.properties).filter(
        (k) => !positionalKeys.has(k),
    );
    if (configKeys.length > 0) {
        parts.push(
            `{ ${configKeys.map((k) => (required.includes(k) ? k : `${k}?`)).join(', ')} }`,
        );
    }
    return `${module.name}(${parts.join(', ')})`;
}

/** A union with more literals than this shows its first few and `…`. */
const MAX_UNION_LITERALS = 4;

/** A module schema as a reference entry. Params and outputs list in the
 *  order their Rust structs declare them. */
export function moduleToReferenceEntry(module: Schema): ReferenceEntry {
    const properties = Object.entries(module.paramsSchema.properties);
    const params = properties.map(([name, schema]): ParamDoc => {
        let type: string | undefined;
        let variants: EnumVariantInfo[] | null = null;
        try {
            type = schemaToTypeExpr(schema, module.paramsSchema);
        } catch {
            // A schema the resolver can't express shows no type.
        }
        try {
            variants = getEnumVariants(schema, module.paramsSchema);
        } catch {
            // Nor any values.
        }
        const described = variants?.some((v) => v.description) ?? false;
        // A long union shortens only where the values are listed below it.
        if (described && variants && variants.length > MAX_UNION_LITERALS) {
            type = `${variants
                .slice(0, 3)
                .map((v) => JSON.stringify(v.rawValue))
                .join(' | ')} | …`;
        }
        return {
            description: (schema as { description?: string }).description,
            meta: paramMeta(module, name),
            name,
            type,
            variants: described ? (variants ?? undefined) : undefined,
        };
    });
    return {
        docs: module.docs,
        experimental: module.experimental,
        name: module.name,
        outputs: module.outputs.map((o) => ({
            description: o.description,
            main: o.default,
            meta: formatOutputMeta(o),
            name: o.name,
        })),
        params,
        signature: moduleSignature(module),
    };
}

/** A description, with the range or default it shows beside it. */
export type DocText = string | { description: string; meta: string };

/** A DSL function's declarations and docs: a global, or a module the DSL
 *  builds from others. */
export interface FunctionDoc {
    /** TypeScript declarations, one per overload, e.g.
     *  `$note(noteName: string): number`. The first names the function and
     *  orders its documented params. */
    declarations: string[];
    /** Keyed by the first declaration's param names. */
    params: Record<string, DocText>;
    returns?: {
        description: string;
        meta?: string;
        /** Members read with a dot, keyed by name with their leading dot. */
        members?: Record<string, DocText>;
    };
    docs: DocContent;
}

/** `text` as a description and its optional meta. */
export function readDocText(text: DocText): {
    description: string;
    meta?: string;
} {
    return typeof text === 'object' ? text : { description: text };
}

/** A DSL function as a reference entry. Its params are the first declaration's,
 *  each typed as every overload types it. */
export function functionToReferenceEntry(fn: FunctionDoc): ReferenceEntry {
    const all = fn.declarations.map(parseDeclaration);
    const [first] = all;
    const params = first.params.map(
        (param): ParamDoc => ({
            ...readDocText(fn.params[param.name]),
            name: param.name,
            rest: param.rest || undefined,
            type: unionOf(
                all.flatMap((d) =>
                    d.params
                        .filter((p) => p.name === param.name)
                        .map((p) => p.types),
                ),
            ),
        }),
    );
    const signature = `${first.name}(${first.params
        .map((p) =>
            p.rest ? `...${p.name}` : p.optional ? `${p.name}?` : p.name,
        )
        .join(', ')})`;
    const returns = fn.returns && {
        description: fn.returns.description,
        members: fn.returns.members
            ? Object.entries(fn.returns.members).map(([name, text]) => ({
                  ...readDocText(text),
                  name,
              }))
            : undefined,
        meta: fn.returns.meta,
        type: unionOf(all.map((d) => d.returns)),
    };
    return {
        docs: fn.docs,
        name: first.name,
        params,
        returns,
        signature,
    };
}

/** Every example in `docs`, sections included. */
export function allExamples(docs: DocContent): DocExample[] {
    return [...docs.examples, ...docs.sections.flatMap((s) => s.examples)];
}

/**
 * A table as one bullet per row, which stays readable in Monaco's narrow
 * hover: `- key — second` with later columns as ` · *column:* cell`.
 */
export function tableToCompactMarkdown(table: DocTable): string[] {
    return table.rows.map((row) => {
        const [key, second, ...rest] = row;
        const extra = rest
            .map((cell, i) => ` · *${table.columns[i + 2]}:* ${cell}`)
            .join('');
        return `- ${key} — ${second}${extra}`;
    });
}

/**
 * The concise in-editor view of `docs` as JSDoc body lines: the description,
 * each section's text and table, and the first example. When the Docs
 * panel holds more (further examples, section examples), a closing link opens
 * it at `name`.
 */
export function renderDocContentJsDoc(
    docs: DocContent,
    name: string,
): string[] {
    const lines: string[] = [...docs.description.split(/\r?\n/)];
    for (const section of docs.sections) {
        lines.push('', `**${section.title}**`);
        if (section.body) {
            lines.push('', ...section.body.split(/\r?\n/));
        }
        if (section.table) {
            lines.push('', ...tableToCompactMarkdown(section.table));
        }
    }
    const [first, ...more] = docs.examples;
    if (first) {
        lines.push(
            '',
            '```js',
            `// ${first.title}`,
            ...first.code.split(/\r?\n/),
            '```',
        );
    }
    const omitted =
        more.length > 0 || docs.sections.some((s) => s.examples.length > 0);
    if (omitted) {
        lines.push('', `More in the docs: {@link ${name}}`);
    }
    return lines;
}
