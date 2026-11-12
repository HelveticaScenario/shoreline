#!/usr/bin/env node

/**
 * CI gate: a PR that changes shipped code (src/, crates/) must add, edit or
 * delete a changie fragment under .changes/unreleased/, unless it carries
 * the `no-changelog` label. Editing or deleting counts because a change to
 * an unreleased feature belongs in that feature's pending entry. Test-only
 * changes are exempt.
 *
 * Env: BASE_SHA (the PR base), LABELS (JSON array of label names).
 */

import { execFileSync } from 'node:child_process';

const base = process.env.BASE_SHA;
if (!base) {
    throw new Error('BASE_SHA is required');
}
const labels = JSON.parse(process.env.LABELS ?? '[]');

const changed = execFileSync(
    'git',
    ['diff', '--name-only', '--diff-filter=ACMRD', `${base}...HEAD`],
    { encoding: 'utf8' },
)
    .split('\n')
    .filter(Boolean);

const isTest = (path) =>
    /(^|\/)(__tests__|tests|e2e|benches|__fixtures__)\//.test(path) ||
    /\.test\.tsx?$/.test(path);
const shipped = changed.filter(
    (path) => /^(src|crates)\//.test(path) && !isTest(path),
);
const fragments = changed.filter((path) =>
    /^\.changes\/unreleased\/.+\.yaml$/.test(path),
);

if (shipped.length === 0) {
    console.log('No shipped code changed; no changelog entry needed.');
} else if (fragments.length > 0) {
    console.log(`Changelog entries: ${fragments.join(', ')}`);
} else if (labels.includes('no-changelog')) {
    console.log('Labelled no-changelog.');
} else {
    console.error(
        'This PR changes shipped code but touches no changelog entry.\n' +
            'Add one with `yarn changelog:new`, or edit the pending entry in\n' +
            '.changes/unreleased/ for an unreleased feature (see CLAUDE.md,\n' +
            '"Changelog"), or\n' +
            'label the PR `no-changelog` if users will not notice the change.',
    );
    process.exit(1);
}
