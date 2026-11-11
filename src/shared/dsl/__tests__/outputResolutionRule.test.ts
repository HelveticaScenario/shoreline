import { describe, expect, test } from 'vitest';
import { chooseOutput, paramTypeClass } from '../outputResolutionRule';

const cycle = [
    { default: true, name: 'cv', outputType: 'cv' },
    { name: 'gate', outputType: 'gate' },
    { name: 'trig', outputType: 'trig' },
    { name: 'velocity', outputType: 'cv' },
];
const clock = [
    { default: true, name: 'playhead', outputType: 'cv' },
    { name: 'barTrigger', outputType: 'trig' },
    { name: 'beatTrigger', outputType: 'trig' },
];
const coinFlip = [
    { default: true, name: 'heads', outputType: 'gate' },
    { name: 'tails', outputType: 'gate' },
];
const lorenz = [
    { default: true, name: 'x', outputType: 'cv' },
    { name: 'y', outputType: 'cv' },
];

describe('paramTypeClass', () => {
    test.each([
        ['audio', 'cv'],
        ['control', 'cv'],
        ['pitch', 'cv'],
        ['frequency', 'cv'],
        [undefined, 'cv'],
        ['gate', 'gate'],
        ['trig', 'trig'],
    ])('%s reads %s', (paramType, expected) => {
        expect(paramTypeClass(paramType)).toBe(expected);
    });
});

describe('chooseOutput', () => {
    test('the default wins when it has the input class', () => {
        expect(chooseOutput(cycle, 'cv')).toBe('cv');
        expect(chooseOutput(coinFlip, 'gate')).toBe('heads');
    });

    test('otherwise the first output of the input class', () => {
        expect(chooseOutput(cycle, 'trig')).toBe('trig');
        expect(chooseOutput(cycle, 'gate')).toBe('gate');
        expect(chooseOutput(clock, 'trig')).toBe('barTrigger');
    });

    test('gate and trig stand in for each other', () => {
        expect(chooseOutput(clock, 'gate')).toBe('barTrigger');
        expect(chooseOutput(coinFlip, 'trig')).toBe('heads');
    });

    test('nothing matching falls back to the default', () => {
        expect(chooseOutput(lorenz, 'trig')).toBe('x');
        expect(chooseOutput(coinFlip, 'cv')).toBe('heads');
    });

    test('with no default flag the first output is the default', () => {
        const outputs = [
            { name: 'a', outputType: 'cv' },
            { name: 'b', outputType: 'trig' },
        ];
        expect(chooseOutput(outputs, 'gate')).toBe('b');
        expect(chooseOutput(outputs, 'cv')).toBe('a');
    });
});
