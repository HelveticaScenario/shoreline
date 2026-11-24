import { useEffect, useRef, useState } from 'react';
import type { ChangelogInfo } from '../../shared/ipcTypes';
import type { ReleaseNotes } from '../../shared/releaseNotes';
import electronAPI from '../electronAPI';
import { ReleaseNotesDialog } from './ReleaseNotesDialog';
import {
    UpdateNotification,
    type UpdateNotificationState,
} from './UpdateNotification';

type ReleaseNotesView =
    | { kind: 'update' }
    | { kind: 'whatsNew'; currentVersion: string; releases: ReleaseNotes[] }
    | { kind: 'changelog'; info: ChangelogInfo };

const IDLE: UpdateNotificationState = { status: 'idle' };

/** The updater's progress, as the main process reports it. */
function useUpdateState() {
    const [state, setState] = useState(IDLE);
    /** The version on offer, which the later progress events do not name. */
    const versionRef = useRef('');

    useEffect(() => {
        const { update } = electronAPI;
        const unsubscribe = [
            update.onAvailable((info) => {
                versionRef.current = info.version;
                setState({
                    currentVersion: info.currentVersion,
                    hasBreakingChanges: info.hasBreakingChanges,
                    releaseNotes: info.releaseNotes,
                    releaseUrl: info.releaseUrl,
                    status: 'available',
                    supportsInAppUpdate: info.supportsInAppUpdate,
                    version: info.version,
                });
            }),
            update.onDownloading(() =>
                setState({
                    status: 'downloading',
                    version: versionRef.current,
                }),
            ),
            update.onPreparing(() =>
                setState({ status: 'preparing', version: versionRef.current }),
            ),
            update.onDownloaded(() => setState({ status: 'ready' })),
            update.onError((message) => setState({ message, status: 'error' })),
        ];
        return () => unsubscribe.forEach((off) => off());
    }, []);

    return {
        dismiss: () => setState(IDLE),
        skip: () => {
            if (versionRef.current) {
                void electronAPI.config.write({
                    skippedUpdateVersion: versionRef.current,
                });
            }
            setState(IDLE);
        },
        state,
    };
}

const download = () => void electronAPI.update.download();
const install = () => void electronAPI.update.install();

/**
 * The update notification and the release notes dialogs: what an offered
 * update changes, what the running version brought on its first launch, and
 * the whole changelog from the menu.
 */
export function AppUpdates() {
    const update = useUpdateState();
    const [view, setView] = useState<ReleaseNotesView | null>(null);
    const close = () => setView(null);

    // The main process records the running version as seen on the first
    // call, so the result is applied even if this effect is torn down
    // meanwhile.
    useEffect(() => {
        electronAPI.releaseNotes
            .whatsNew()
            .then((info) => {
                if (info.releaseNotes.length > 0) {
                    setView({
                        currentVersion: info.currentVersion,
                        kind: 'whatsNew',
                        releases: info.releaseNotes,
                    });
                }
            })
            .catch((err) => {
                console.warn("[release-notes] What's new failed:", err);
            });
        return electronAPI.releaseNotes.onMenuShow(() => {
            electronAPI.releaseNotes
                .getAll()
                .then((info) => setView({ info, kind: 'changelog' }))
                .catch((err) => {
                    console.warn('[release-notes] Changelog failed:', err);
                });
        });
    }, []);

    const { state } = update;
    return (
        <>
            <UpdateNotification
                state={state}
                onDownload={download}
                onInstall={install}
                onSkip={update.skip}
                onDismiss={update.dismiss}
                onShowReleaseNotes={() => setView({ kind: 'update' })}
            />
            {view?.kind === 'update' && state.status === 'available' && (
                <ReleaseNotesDialog
                    title={`Version ${state.version} is available`}
                    summary={`You have version ${state.currentVersion}. Here is everything that changed since then.`}
                    releases={state.releaseNotes}
                    actions={[
                        {
                            label: state.supportsInAppUpdate
                                ? 'Download & Install'
                                : 'View Release',
                            onClick: () => {
                                close();
                                download();
                            },
                            primary: true,
                        },
                        {
                            label: 'Skip This Version',
                            onClick: () => {
                                close();
                                update.skip();
                            },
                        },
                        { label: 'Not Now', onClick: close },
                    ]}
                    onClose={close}
                />
            )}
            {view?.kind === 'whatsNew' && (
                <ReleaseNotesDialog
                    title={`What's new in version ${view.currentVersion}`}
                    releases={view.releases}
                    onClose={close}
                />
            )}
            {view?.kind === 'changelog' && (
                <ReleaseNotesDialog
                    title="Release Notes"
                    releases={view.info.releaseNotes}
                    liftBreaking={false}
                    installedVersion={view.info.currentVersion}
                    link={{
                        href: view.info.fullChangelogUrl,
                        label: 'Full changelog on GitHub',
                    }}
                    onClose={close}
                />
            )}
        </>
    );
}
