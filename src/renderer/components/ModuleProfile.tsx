import { useEffect, useState } from 'react';
import type { ModuleProfileSample } from '@modular/core';
import type { SortingState } from '@tanstack/react-table';
import electronAPI from '../electronAPI';
import { useOverlayPanel } from '../hooks/useOverlayPanel';
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
    const [rows, setRows] = useState<ModuleProfileSample[] | null>(null);
    const [sorting, setSorting] = useState<SortingState>([
        { id: 'self', desc: true },
    ]);
    const [sampleRate, setSampleRate] = useState(16);
    const [unit, setUnit] = useState<Unit>('ns');
    const [sampleRateHz, setSampleRateHz] = useState(48000);
    const [bufferSize, setBufferSize] = useState(256);
    const panelRef = useOverlayPanel(isOpen, onClose);

    useEffect(() => {
        if (!isOpen) return;
        electronAPI.audio
            .getCurrentState()
            .then((s) => {
                setSampleRateHz(s.sampleRate);
                if (s.bufferSize && s.bufferSize > 0) {
                    setBufferSize(s.bufferSize);
                }
            })
            .catch(console.error);
    }, [isOpen]);

    useEffect(() => {
        if (!isOpen) return;
        let cancelled = false;
        let intervalId: ReturnType<typeof setInterval> | null = null;

        const poll = () => {
            electronAPI.synthesizer
                .getModuleProfile()
                .then((data) => {
                    if (!cancelled) setRows(data);
                })
                .catch(console.error);
        };

        // Enable, then poll. Keyed on `isOpen` only: the sampling rate is
        // set by the effect below, so changing it never tears down and
        // re-enables profiling. The enable refcounts on the Rust side
        // (AudioState), so concurrent consumers remain safe if this re-runs.
        void (async () => {
            try {
                await electronAPI.synthesizer.setModuleProfilingEnabled(true);
                // The cleanup always issues the balancing disable, so just
                // bail here. Disabling again would double-decrement the
                // refcounted enable on the Rust side.
                if (cancelled) return;
                poll();
                intervalId = setInterval(poll, 1000);
            } catch (err) {
                console.error(err);
            }
        })();

        return () => {
            cancelled = true;
            if (intervalId !== null) clearInterval(intervalId);
            electronAPI.synthesizer
                .setModuleProfilingEnabled(false)
                .catch(console.error);
            setRows(null);
        };
    }, [isOpen]);

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

    const data = rows ? toDisplayRows(rows) : [];
    const columns = profileColumns(data, unit, sampleRateHz);

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
                    {rows === null ? (
                        <div className="module-profile-loading">Loading…</div>
                    ) : data.length === 0 ? (
                        <div className="module-profile-loading">
                            No module activity yet.
                        </div>
                    ) : (
                        <ModuleProfileTable
                            data={data}
                            columns={columns}
                            sorting={sorting}
                            onSortingChange={setSorting}
                        />
                    )}
                </div>
            </div>
        </div>
    );
}
