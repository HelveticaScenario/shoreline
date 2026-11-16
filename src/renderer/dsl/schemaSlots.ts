/**
 * Slots inside a module param's value — an element of a poly array, a
 * tuple position, a member of a nested object (`$eq`'s `sections`) — read
 * from the param's JSON schema.
 */
import type { SlotStep } from './apiTypeSlots';

type Json = Record<string, unknown>;

/** The `$defs` that hold a signal. A `PolySignal` or `LaneSum` is also an
 *  array of `Signal`s. */
const SIGNAL_DEFS = new Set(['Signal', 'PolySignal', 'LaneSum']);

const MAX_DEPTH = 32;

const isJson = (v: unknown): v is Json =>
    typeof v === 'object' && v !== null && !Array.isArray(v);

const refName = (s: Json): string | null =>
    typeof s.$ref === 'string' && s.$ref.startsWith('#/$defs/')
        ? s.$ref.slice('#/$defs/'.length)
        : null;

/** The `#[input]` metadata a nested signal member carries as `x-signal`. */
export interface SignalMeta {
    paramType: string;
    defaultValue?: number;
    minValue?: number;
    maxValue?: number;
    controlMin?: number;
    controlMax?: number;
}

/** A slot at a path inside a param, with the deepest member on the path
 *  that carries `#[input]` metadata, or else the description of the deepest
 *  one that has a description (null when none does). */
export interface SchemaSlot {
    description: string | null;
    meta: SignalMeta | null;
}

/**
 * The slot at `path` inside a value of `schema`, or null when it holds no
 * signal. `defs` resolves `$ref`s.
 */
export function schemaSlot(
    schema: unknown,
    path: readonly SlotStep[],
    defs: Json,
    depth = 0,
): SchemaSlot | null {
    if (!isJson(schema) || depth > MAX_DEPTH) {
        return null;
    }
    if (path.length === 0) {
        return holdsSignal(schema, defs, depth)
            ? { description: null, meta: null }
            : null;
    }
    const [step, ...rest] = path;
    for (const child of children(schema, step, defs, depth)) {
        const found = schemaSlot(child, rest, defs, depth + 1);
        if (found) {
            if (found.meta || !('member' in step)) {
                return found;
            }
            const meta = signalMeta(child['x-signal']);
            if (meta) {
                return { description: null, meta };
            }
            const own =
                typeof child.description === 'string'
                    ? child.description
                    : null;
            return { description: found.description ?? own, meta: null };
        }
    }
    return null;
}

function signalMeta(v: unknown): SignalMeta | null {
    return isJson(v) && typeof v.paramType === 'string'
        ? (v as unknown as SignalMeta)
        : null;
}

function holdsSignal(s: Json, defs: Json, depth: number): boolean {
    if (depth > MAX_DEPTH) {
        return false;
    }
    const ref = refName(s);
    if (ref !== null) {
        return SIGNAL_DEFS.has(ref) || holdsSignal(def(defs, ref), defs, depth);
    }
    return branches(s).some((b) => holdsSignal(b, defs, depth + 1));
}

/** The schemas `step` reaches inside a value of `s`. */
function children(s: Json, step: SlotStep, defs: Json, depth: number): Json[] {
    if (depth > MAX_DEPTH) {
        return [];
    }
    const ref = refName(s);
    if (ref !== null) {
        if (SIGNAL_DEFS.has(ref)) {
            return ref !== 'Signal' && 'element' in step
                ? [{ $ref: '#/$defs/Signal' }]
                : [];
        }
        return children(def(defs, ref), step, defs, depth + 1);
    }
    const out = branches(s).flatMap((b) => children(b, step, defs, depth + 1));
    if ('element' in step) {
        const prefix = Array.isArray(s.prefixItems) ? s.prefixItems : null;
        const item = prefix ? prefix[step.element] : s.items;
        if (isJson(item)) {
            out.push(item);
        }
    } else if (isJson(s.properties)) {
        const member = s.properties[step.member];
        if (isJson(member)) {
            out.push(member);
        }
    }
    return out;
}

function branches(s: Json): Json[] {
    return [s.anyOf, s.oneOf, s.allOf].flatMap((list) =>
        Array.isArray(list) ? list.filter(isJson) : [],
    );
}

function def(defs: Json, name: string): Json {
    const d = defs[name];
    return isJson(d) ? d : {};
}
