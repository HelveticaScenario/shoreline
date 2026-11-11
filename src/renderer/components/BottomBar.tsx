import { AudioLines, BookOpen, type LucideIcon } from 'lucide-react';
import type { BottomTab, DocsState } from '../hooks/useBottomPanel';
import './BottomBar.css';

const TABS: { id: BottomTab; label: string; icon: LucideIcon }[] = [
    { id: 'mixer', label: 'Mixer', icon: AudioLines },
    { id: 'docs', label: 'Docs', icon: BookOpen },
];

interface BottomBarProps {
    tab: BottomTab | null;
    onToggle: (tab: BottomTab) => void;
    docsLocation: DocsState['location'];
}

/** The bar along the window's foot that opens the panels under the editor. */
export function BottomBar({ tab, onToggle, docsLocation }: BottomBarProps) {
    const tabs =
        docsLocation === 'bottom'
            ? TABS
            : TABS.filter(({ id }) => id !== 'docs');
    return (
        <nav className="app-bottom-bar" aria-label="Bottom panels">
            {tabs.map(({ id, label, icon: Icon }) => (
                <button
                    key={id}
                    aria-pressed={tab === id}
                    aria-controls="app-bottom-panel"
                    aria-label={label}
                    title={label}
                    className={`app-bottom-bar-item ${id} ${tab === id ? 'active' : ''}`}
                    onClick={() => onToggle(id)}
                >
                    <Icon size={22} strokeWidth={1.5} aria-hidden />
                </button>
            ))}
        </nav>
    );
}
