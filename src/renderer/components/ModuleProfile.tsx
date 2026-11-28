import { useEffect, useState } from 'react';
import type { OnChangeFn, SortingState } from '@tanstack/react-table';
import electronAPI from '../electronAPI';
import { useOverlayPanel } from '../hooks/useOverlayPanel';
import { useAudioState, useModuleProfile } from '../queries/engine';
import { ModuleProfileTable } from './moduleProfile/ModuleProfileTable';
import {
    profileColumns,
    toDisplayRows,
    type Unit,
} from './moduleProfile/columns';
import './ModuleProfile.css';

interface ModuleProfileProps {
    isOpen: boolean;
    onClose: () => void;
}

const SAMPLE_RATE_OPTIONS = [
    { label: 'Every callback', value: 1 },
    { label: '1 in 4', value: 4 },
    { label: '1 in 16', value: 16 },
    { label: '1 in 64', value: 64 },
];

export function ModuleProfile({ isOpen, onClose }: ModuleProfileProps) {
    const [sorting, setSorting] = useState<SortingState>([
        { id: 'self', desc: true },
    ]);
    const [sampleRate, setSampleRate] = useState(16);
    const [unit, setUnit] = useState<Unit>('ns');
    const { data: audioState } = useAudioState(isOpen);
    const sampleRateHz = audioState?.sampleRate ?? 48000;
    const bufferSize =
        audioState?.bufferSize && audioState.bufferSize > 0
            ? audioState.bufferSize
            : 256;
    const panelRef = useOverlayPanel(isOpen, onClose);

    // The dropdown sets how many audio callbacks the engine times (1 in
    // `sampleRate`). That setting is separate from the enable refcount and
    // the polling loop, so a change never toggles profiling off and on
    // (which would blip the global and flash "Loading…").
    useEffect(() => {
        if (!isOpen) return;
        electronAPI.synthesizer
            .setModuleProfilingSampleRate(sampleRate)
            .catch(console.error);
    }, [isOpen, sampleRate]);

    if (!isOpen) return null;

    return (
        <div className="module-profile-overlay" onClick={onClose}>
            <div
                className="module-profile-panel"
                ref={panelRef}
                tabIndex={-1}
                onClick={(e) => e.stopPropagation()}
            >
                <div className="module-profile-header">
                    <h2>Module Profile</h2>
                    <div className="module-profile-controls">
                        <div
                            className="module-profile-unit-toggle"
                            role="group"
                            aria-label="Display unit"
                        >
                            <button
                                type="button"
                                className={unit === 'ns' ? 'active' : ''}
                                onClick={() => setUnit('ns')}
                            >
                                ns
                            </button>
                            <button
                                type="button"
                                className={unit === 'pct' ? 'active' : ''}
                                onClick={() => setUnit('pct')}
                                title={`% of audio callback budget — ${bufferSize} frames at ${sampleRateHz} Hz = ${((bufferSize * 1e6) / sampleRateHz).toFixed(2)} ms per callback`}
                            >
                                %
                            </button>
                        </div>
                        <label className="module-profile-rate-label">
                            Sample
                            <select
                                className="module-profile-rate-select"
                                value={sampleRate}
                                onChange={(e) =>
                                    setSampleRate(Number(e.target.value))
                                }
                            >
                                {SAMPLE_RATE_OPTIONS.map((opt) => (
                                    <option key={opt.value} value={opt.value}>
                                        {opt.label}
                                    </option>
                                ))}
                            </select>
                        </label>
                        <button
                            className="module-profile-close-btn"
                            onClick={onClose}
                            aria-label="Close"
                        >
                            ×
                        </button>
                    </div>
                </div>

                <div className="module-profile-body">
                    <p className="module-profile-legend">
                        Per-sample averages. <strong>Self</strong>: time in this
                        module's own DSP. <strong>Params</strong>: time fetching
                        cable inputs from upstream (their cost shows in their
                        own Self). <strong>Total</strong>: Self + Params.{' '}
                        <strong>Mode</strong>: <em>sample</em> = in a feedback
                        cycle (per-sample wrapper); <em>block</em> = acyclic
                        (per-block wrapper).
                    </p>
                    <ModuleProfileRows
                        sampleRateHz={sampleRateHz}
                        sorting={sorting}
                        onSortingChange={setSorting}
                        unit={unit}
                    />
                </div>
            </div>
        </div>
    );
}

/**
 * The profiled modules. Mounted only while the panel is open: it turns
 * profiling on for as long as it is mounted, and each opening reads afresh.
 */
function ModuleProfileRows({
    sampleRateHz,
    sorting,
    onSortingChange,
    unit,
}: {
    sampleRateHz: number;
    sorting: SortingState;
    onSortingChange: OnChangeFn<SortingState>;
    unit: Unit;
}) {
    const [profiling, setProfiling] = useState(false);
    const { data: rows } = useModuleProfile(profiling);

    // The enable refcounts on the Rust side (AudioState), so concurrent
    // consumers stay safe if this re-runs. The cleanup always issues the
    // balancing disable, so an enable that lands after unmount must not
    // disable again: that would double-decrement the refcount.
    useEffect(() => {
        let cancelled = false;
        electronAPI.synthesizer
            .setModuleProfilingEnabled(true)
            .then(() => {
                if (!cancelled) setProfiling(true);
            })
            .catch(console.error);
        return () => {
            cancelled = true;
            electronAPI.synthesizer
                .setModuleProfilingEnabled(false)
                .catch(console.error);
        };
    }, []);

    if (rows === undefined) {
        return <div className="module-profile-loading">Loading…</div>;
    }
    const data = toDisplayRows(rows);
    if (data.length === 0) {
        return (
            <div className="module-profile-loading">
                No module activity yet.
            </div>
        );
    }
    return (
        <ModuleProfileTable
            data={data}
            columns={profileColumns(data, unit, sampleRateHz)}
            sorting={sorting}
            onSortingChange={onSortingChange}
        />
    );
}
