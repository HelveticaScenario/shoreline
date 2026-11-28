import { useEngineHealth } from '../queries/engine';
import { useOverlayPanel } from '../hooks/useOverlayPanel';
import './EngineHealth.css';

interface EngineHealthProps {
    isOpen: boolean;
    onClose: () => void;
}

function usageClass(usage: number): string {
    if (usage >= 0.8) return 'danger';
    if (usage >= 0.5) return 'warning';
    return '';
}

function formatUsage(usage: number): string {
    return `${(usage * 100).toFixed(1)}%`;
}

function formatNs(ns: number): string {
    return `${ns.toFixed(0)} ns/sample`;
}

export function EngineHealth({ isOpen, onClose }: EngineHealthProps) {
    const panelRef = useOverlayPanel(isOpen, onClose);

    if (!isOpen) return null;

    return (
        <div className="engine-health-overlay" onClick={onClose}>
            <div
                className="engine-health-panel"
                ref={panelRef}
                tabIndex={-1}
                onClick={(e) => e.stopPropagation()}
            >
                <div className="engine-health-header">
                    <h2>Engine Health</h2>
                    <button
                        className="engine-health-close-btn"
                        onClick={onClose}
                        aria-label="Close"
                    >
                        ×
                    </button>
                </div>

                <div className="engine-health-body">
                    <EngineHealthBody />
                </div>
            </div>
        </div>
    );
}

/** Mounted only while the panel is open, so each opening reads afresh. */
function EngineHealthBody() {
    const { data: snapshot } = useEngineHealth();
    if (snapshot === undefined) {
        return <div className="engine-health-loading">Loading…</div>;
    }
    return (
        <div className="engine-health-section">
            <p className="engine-health-section-title">Audio CPU</p>
            <div className="engine-health-row">
                <span className="engine-health-label">Average</span>
                <div className="engine-health-values">
                    <span
                        className={`engine-health-usage ${usageClass(snapshot.avgUsage)}`}
                    >
                        {formatUsage(snapshot.avgUsage)}
                    </span>
                    <span className="engine-health-ns">
                        {formatNs(snapshot.avgNsPerSample)}
                    </span>
                </div>
            </div>
            <div className="engine-health-row">
                <span className="engine-health-label">Peak</span>
                <div className="engine-health-values">
                    <span
                        className={`engine-health-usage ${usageClass(snapshot.peakUsage)}`}
                    >
                        {formatUsage(snapshot.peakUsage)}
                    </span>
                    <span className="engine-health-ns">
                        {formatNs(snapshot.peakNsPerSample)}
                    </span>
                </div>
            </div>
        </div>
    );
}
