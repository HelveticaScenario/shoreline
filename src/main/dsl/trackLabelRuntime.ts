/**
 * Runs a patch's track labels. Each top-level `name: expr` becomes an
 * assignment to a hidden global whose setter registers the track, and each
 * track's name becomes a global the rest of the patch reads.
 *
 * The rewrite moves no character the patch wrote: the label and its colon
 * are overwritten in place, one for one, with line breaks kept. Every source
 * position the executor captures from V8 (call sites, controls, scopes)
 * therefore reads the same on the rewritten text as on the patch.
 */
import ts from 'typescript';
import vm from 'node:vm';

import { parseDsl } from '../../shared/dsl/ast/parse';
import { RECEIVER_SLOT } from '../../shared/dsl/outputResolutionRule';
import { callSiteKey, lineColumn } from '../../shared/dsl/ast/position';
import { FIRST_LINE_COLUMN_OFFSET } from '../../shared/dsl/spanTypes';
import {
    analyzeTrackLabels,
    type TrackStatement,
} from '../../shared/dsl/trackLabels';
import { captureSourceLocation } from './captureSourceLocation';
import type { GraphBuilder } from './GraphBuilder';
import { createTrackTap, type TrackTap } from './trackTap';

/**
 * The hidden global a label is overwritten with. One UTF-16 unit, so the
 * shortest label (`a:`) has room for it and the `=` after it.
 */
const ACCESSOR = 'ꙮ';

const LINE_BREAK = /[\n\r\u2028\u2029]/;

export interface TrackLabelProgram {
    /** The patch with its labels overwritten, ready to run. */
    runSource: string;
    /** Define the accessor and the track names on the patch's global. */
    install(sandbox: vm.Context, builder: GraphBuilder): void;
}

/** Where V8 reports `pos` for the running patch, as captured. */
function capturedLocation(
    file: ts.SourceFile,
    pos: number,
): { line: number; column: number } {
    const { line, column } = lineColumn(file, pos);
    return {
        column: column + (line === 1 ? FIRST_LINE_COLUMN_OFFSET : 0),
        line,
    };
}

/**
 * Read `source`'s track labels, throwing at the first that breaks the rules.
 */
export function prepareTrackLabels(source: string): TrackLabelProgram {
    const file = parseDsl(source);
    const { tracks, errors } = analyzeTrackLabels(ts, file);
    if (errors.length > 0) {
        const first = errors[0];
        throw new Error(
            `${first.message} (line ${lineColumn(file, first.start).line})`,
        );
    }
    if (tracks.length > 0 && source.includes(ACCESSOR)) {
        throw new Error(
            `a patch with track labels cannot use the character ${ACCESSOR}`,
        );
    }

    let runSource = '';
    let copied = 0;
    // V8 places an assignment at its `=`, just after the accessor.
    const byAssignment = new Map<string, TrackStatement>();
    for (const statement of tracks) {
        const { labelStart, colonEnd } = statement;
        const region = source.slice(labelStart, colonEnd);
        if (LINE_BREAK.test(region[1])) {
            throw new Error(
                `write a track label and its colon on one line (line ${lineColumn(file, labelStart).line})`,
            );
        }
        // Unit by unit, so every UTF-16 unit keeps its column.
        const blanked = region.slice(2).replace(/[^\n\r\u2028\u2029]/g, ' ');
        runSource +=
            source.slice(copied, labelStart) +
            ACCESSOR +
            '=' +
            // A label's first two units are never a line break.
            blanked;
        copied = colonEnd;
        byAssignment.set(callSiteKey(file, labelStart + 1), statement);
    }
    runSource += source.slice(copied);

    const install = (sandbox: vm.Context, builder: GraphBuilder): void => {
        const builtins = new Set(
            vm.runInContext(
                'Object.getOwnPropertyNames(globalThis)',
                sandbox,
            ) as string[],
        );
        for (const { track, labelStart } of tracks) {
            const { name } = track;
            if (name in sandbox || builtins.has(name)) {
                throw new Error(
                    `track ${name} has the name of a DSL global, so it cannot be bound (line ${lineColumn(file, labelStart).line})`,
                );
            }
            let tap: TrackTap | undefined;
            Object.defineProperty(sandbox, name, {
                configurable: false,
                enumerable: false,
                // The name reads as the track's tap, whether or not its
                // label has run; `sends` takes it to reach a bus track.
                get: () => {
                    tap ??= createTrackTap(builder, name);
                    return tap;
                },
                set: () => {
                    throw new Error(
                        `track ${name} is bound by its label and cannot be assigned`,
                    );
                },
            });
        }
        Object.defineProperty(sandbox, ACCESSOR, {
            configurable: false,
            enumerable: false,
            set: (value: unknown) => {
                const at = captureSourceLocation();
                const statement =
                    at && byAssignment.get(`${at.line}:${at.column}`);
                if (!statement) {
                    throw new Error(
                        `${ACCESSOR} is reserved for track labels and cannot be assigned`,
                    );
                }
                const { track, labelStart } = statement;
                // A module root read as the track's signal is recorded here,
                // as a chain method records its receiver.
                builder.outputResolver.withCall(
                    [[RECEIVER_SLOT, value, 'cv']],
                    () =>
                        builder.addTrack(
                            track,
                            value,
                            capturedLocation(file, labelStart),
                        ),
                );
            },
        });
    };

    return { install, runSource };
}
