import { execFileSync } from 'node:child_process';
import { expect, test } from 'vitest';

// React Compiler leaves a component it cannot handle uncompiled without
// failing the build, so the script is the only signal that one lost its
// memoization.
test('React Compiler compiles every renderer component and hook', () => {
    const output = execFileSync(
        process.execPath,
        ['scripts/check-react-compiler.mjs'],
        { encoding: 'utf8' },
    );
    expect(output).toMatch(/compiled \d+ components and hooks/);
}, 60_000);
