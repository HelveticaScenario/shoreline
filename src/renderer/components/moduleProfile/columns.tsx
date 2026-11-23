import type { ModuleProfileSample } from '@modular/core';
import { createColumnHelper } from '@tanstack/react-table';

export type Unit = 'ns' | 'pct';

export interface DisplayRow {
    moduleId: string;
    mode: string;
    selfNsPerSample: number;
    paramsNsPerSample: number;
    totalNsPerSample: number;
}

function nsPerSample(ns: number, samples: number): number {
    return samples > 0 ? ns / samples : 0;
}

export function toDisplayRows(rows: ModuleProfileSample[]): DisplayRow[] {
    return rows.map((r) => {
        const self = nsPerSample(r.selfNs, r.samplesProcessed);
        const total = nsPerSample(r.totalNs, r.samplesProcessed);
        // Clamp to 0: clock granularity and timer skew between
        // push_frame and pop_frame can briefly push self past total.
        const params = Math.max(0, total - self);
        return {
            moduleId: r.moduleId,
            mode: r.mode,
            selfNsPerSample: self,
            paramsNsPerSample: params,
            totalNsPerSample: total,
        };
    });
}

function formatValue(
    valueNsPerSample: number,
    unit: Unit,
    sampleRateHz: number,
): string {
    const v = valueNsPerSample;
    if (unit === 'pct') {
        if (sampleRateHz <= 0) return '—';
        // Share of the per-sample real-time budget (1e9 / sampleRateHz ns
        // per sample). Buffer size cancels out of the callback-level ratio,
        // so it has no effect here.
        const pct = ((v * sampleRateHz) / 1e9) * 100;
        if (pct >= 10) return `${pct.toFixed(1)}%`;
        if (pct >= 0.01) return `${pct.toFixed(2)}%`;
        return pct === 0 ? '0%' : '<0.01%';
    }
    if (v >= 10000) return `${(v / 1000).toFixed(1)} µs`;
    if (v >= 100) return `${v.toFixed(0)} ns`;
    if (v === 0) return '0 ns';
    return `${v.toFixed(1)} ns`;
}

function formatBar(value: number, max: number): string {
    if (max <= 0) return '0';
    return `${Math.min(100, (value / max) * 100).toFixed(0)}%`;
}

const columnHelper = createColumnHelper<DisplayRow>();

/** The table's columns, showing times in `unit` and each Self bar against the largest Self in `data`. */
export function profileColumns(
    data: DisplayRow[],
    unit: Unit,
    sampleRateHz: number,
) {
    const maxSelfNsPerSample = data.reduce(
        (max, r) => Math.max(max, r.selfNsPerSample),
        0,
    );
    return [
        columnHelper.accessor('moduleId', {
            id: 'module',
            header: 'Module',
            cell: (info) => (
                <span title={info.getValue()}>{info.getValue()}</span>
            ),
            sortingFn: (a, b) =>
                a.original.moduleId.localeCompare(b.original.moduleId),
        }),
        columnHelper.accessor('mode', {
            id: 'mode',
            header: 'Mode',
            cell: (info) => {
                const m = info.getValue();
                return (
                    <span
                        className={`module-profile-mode module-profile-mode-${m}`}
                        title={
                            m === 'sample'
                                ? 'Sample mode: in a feedback cycle, processes one sample per ensure_processed_to call. Higher wrapper overhead per sample.'
                                : 'Block mode: acyclic subgraph, processes a full block per call. Lower per-sample overhead.'
                        }
                    >
                        {m}
                    </span>
                );
            },
            sortingFn: (a, b) => a.original.mode.localeCompare(b.original.mode),
        }),
        columnHelper.accessor('selfNsPerSample', {
            id: 'self',
            header: 'Self',
            cell: (info) => {
                const v = info.getValue();
                return (
                    <div className="module-profile-cell">
                        <span className="module-profile-bar">
                            <span
                                className="module-profile-bar-fill"
                                style={{
                                    width: formatBar(v, maxSelfNsPerSample),
                                }}
                            />
                        </span>
                        <span>{formatValue(v, unit, sampleRateHz)}</span>
                    </div>
                );
            },
            sortingFn: (a, b) =>
                a.original.selfNsPerSample - b.original.selfNsPerSample,
        }),
        columnHelper.accessor('paramsNsPerSample', {
            id: 'params',
            header: 'Params',
            cell: (info) => formatValue(info.getValue(), unit, sampleRateHz),
            sortingFn: (a, b) =>
                a.original.paramsNsPerSample - b.original.paramsNsPerSample,
        }),
        columnHelper.accessor('totalNsPerSample', {
            id: 'total',
            header: 'Total',
            cell: (info) => formatValue(info.getValue(), unit, sampleRateHz),
            sortingFn: (a, b) =>
                a.original.totalNsPerSample - b.original.totalNsPerSample,
        }),
    ];
}

export type ProfileColumns = ReturnType<typeof profileColumns>;
