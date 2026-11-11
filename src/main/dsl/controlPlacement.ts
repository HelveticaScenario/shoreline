import { parseDsl } from '../../shared/dsl/ast/parse';
import { callSiteKey } from '../../shared/dsl/ast/position';
import { findControlCalls } from '../../shared/dsl/controlCalls';

/**
 * Reject any control or group the static scan does not place where
 * evaluation put it. The control panel builds its controls and groups from
 * the source, so one whose group argument it cannot resolve — a group
 * passed as a parameter or held in a `let` — or one made through an aliased
 * factory would be missing from the panel, and one it resolves differently
 * would show in the wrong group.
 */
export function assertControlsPlaced(
    source: string,
    controls: Array<{
        label: string;
        group: string[];
        sourceLocation?: { line: number; column: number };
    }>,
): void {
    const file = parseDsl(source);
    // Keys in V8 call-site coordinates, matching captured source locations.
    const placed = new Map(
        findControlCalls(file).map(
            ({ name, group }) =>
                [callSiteKey(file, name.getStart(file)), group] as const,
        ),
    );
    for (const { label, group, sourceLocation: loc } of controls) {
        if (!loc) {
            continue;
        }
        const scanned = placed.get(`${loc.line}:${loc.column}`);
        if (!scanned) {
            throw new Error(
                `Control "${label}" at line ${loc.line} must be created by a direct $slider/$btn/$toggleBtn/$cGroup call whose group, if given, is a $cGroup(...) call or a const bound to one ($cGroup params written as an object literal), so the Control panel can place it`,
            );
        }
        if (JSON.stringify(scanned) !== JSON.stringify(group)) {
            throw new Error(
                `Control "${label}" at line ${loc.line} is in group "${group.join(' / ')}" when evaluated, but the Control panel would place it in "${scanned.join(' / ')}" — pass the group as a $cGroup(...) call or a const bound to one that no other binding shadows`,
            );
        }
    }
}
