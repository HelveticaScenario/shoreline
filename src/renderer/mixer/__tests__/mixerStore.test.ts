import { expect, test } from 'vitest';

import { mixerOn, track } from './mixerHarness';

test('a folded group stays folded through patch updates', () => {
    const { mixer } = mixerOn('', [track('drums', { isGroup: true })]);
    mixer.store.toggleFold('drums');
    expect(mixer.store.getSnapshot().folded.has('drums')).toBe(true);
    mixer.commit([track('drums', { isGroup: true })], null, null);
    expect(mixer.store.getSnapshot().folded.has('drums')).toBe(true);
    mixer.store.toggleFold('drums');
    expect(mixer.store.getSnapshot().folded.has('drums')).toBe(false);
});

test('a fold notifies subscribers', () => {
    const { mixer } = mixerOn('', []);
    let notified = 0;
    mixer.store.subscribe(() => notified++);
    mixer.store.toggleFold('drums');
    expect(notified).toBe(1);
});
