/**
 * True for a plain `{ ... }` literal. Patch scripts evaluate in a vm realm
 * whose `Object.prototype` is a different object from the host's, so
 * plainness is judged by chain depth (a prototype that is itself the chain
 * root) rather than identity.
 */
export function isPlainObject(v: unknown): v is Record<string, unknown> {
    if (typeof v !== 'object' || v === null || Array.isArray(v)) {
        return false;
    }
    const proto = Object.getPrototypeOf(v);
    return proto === null || Object.getPrototypeOf(proto) === null;
}
