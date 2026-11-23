import { SelectSetting } from './SettingsControls';
import { useAudioSettings } from './audioSettings/useAudioSettings';

interface AudioSettingsTabProps {
    isActive: boolean;
}

/**
 * Audio device choices. They take effect on Apply, since reopening the
 * streams interrupts the sound. An output selection of `null` follows the
 * system default output.
 */
export function AudioSettingsTab({ isActive }: AudioSettingsTabProps) {
    const {
        applyConfig,
        applying,
        error,
        hosts,
        isDirty,
        loading,
        options,
        refreshDeviceList,
        select,
        selection,
        unlistedOutputName,
        warning,
    } = useAudioSettings(isActive);
    const {
        bufferSizes,
        followedOutputDevice,
        inputDevices,
        outputDevices,
        sampleRates,
        selectedInputDevice,
        selectedOutputDevice,
        unlistedOutputDeviceId,
    } = options;

    return (
        <div className="settings-tab-content">
            {error && <div className="settings-tab-error">{error}</div>}

            {warning && <div className="settings-tab-warning">{warning}</div>}

            {loading ? (
                <div className="settings-tab-loading">Loading devices...</div>
            ) : (
                <>
                    <SelectSetting
                        label="Audio Host"
                        onApproach={refreshDeviceList}
                        value={selection.hostId || ''}
                        onChange={(value) => select({ hostId: value || null })}
                    >
                        {hosts.map((h) => (
                            <option key={h.hostId} value={h.hostId}>
                                {h.hostName}
                            </option>
                        ))}
                    </SelectSetting>

                    <SelectSetting
                        label="Output Device"
                        onApproach={refreshDeviceList}
                        value={selection.outputDeviceId || ''}
                        onChange={(value) =>
                            select({ outputDeviceId: value || null })
                        }
                        note={
                            selectedOutputDevice && (
                                <div className="device-info">
                                    {selectedOutputDevice.sampleRate} Hz,{' '}
                                    {selectedOutputDevice.outputChannels}{' '}
                                    channels
                                </div>
                            )
                        }
                    >
                        <option value="">
                            System Default
                            {followedOutputDevice &&
                            !followedOutputDevice.followsDefault
                                ? ` (${followedOutputDevice.name})`
                                : ''}
                        </option>
                        <optgroup label="Devices">
                            {unlistedOutputDeviceId && (
                                <option value={unlistedOutputDeviceId}>
                                    {unlistedOutputName} (not connected)
                                </option>
                            )}
                            {outputDevices
                                .filter((d) => !d.followsDefault)
                                .map((d) => (
                                    <option key={d.id} value={d.id}>
                                        {d.name} ({d.outputChannels} ch)
                                    </option>
                                ))}
                        </optgroup>
                    </SelectSetting>

                    <SelectSetting
                        label="Input Device"
                        onApproach={refreshDeviceList}
                        value={selection.inputDeviceId || ''}
                        onChange={(value) =>
                            select({ inputDeviceId: value || null })
                        }
                        note={
                            selectedInputDevice && (
                                <div className="device-info">
                                    {selectedInputDevice.sampleRate} Hz,{' '}
                                    {selectedInputDevice.inputChannels} channels
                                </div>
                            )
                        }
                    >
                        <option value="">None (No Input)</option>
                        {inputDevices.map((d) => (
                            <option key={d.id} value={d.id}>
                                {d.name} ({d.inputChannels} ch)
                                {d.isDefault && !d.followsDefault
                                    ? ' (Default)'
                                    : ''}
                            </option>
                        ))}
                    </SelectSetting>

                    <SelectSetting
                        label="Sample Rate"
                        value={selection.sampleRate || ''}
                        onChange={(value) =>
                            select({ sampleRate: Number(value) || null })
                        }
                        disabled={sampleRates.length === 0}
                    >
                        {sampleRates.length === 0 ? (
                            <option value="">Select devices first</option>
                        ) : (
                            sampleRates.map((rate) => (
                                <option key={rate} value={rate}>
                                    {rate} Hz
                                </option>
                            ))
                        )}
                    </SelectSetting>

                    <SelectSetting
                        label="Buffer Size"
                        value={selection.bufferSize || ''}
                        onChange={(value) =>
                            select({ bufferSize: Number(value) || null })
                        }
                        disabled={bufferSizes.length === 0}
                    >
                        {bufferSizes.length === 0 ? (
                            <option value="">Select devices first</option>
                        ) : (
                            bufferSizes.map((size) => (
                                <option key={size} value={size}>
                                    {size} samples (~
                                    {(
                                        (size /
                                            (selection.sampleRate || 48000)) *
                                        1000
                                    ).toFixed(1)}{' '}
                                    ms)
                                </option>
                            ))
                        )}
                    </SelectSetting>

                    <div className="settings-section settings-row">
                        <span className="action-spacer" />
                        <button
                            className="btn btn-primary"
                            onClick={applyConfig}
                            disabled={applying || !isDirty}
                        >
                            {applying ? 'Applying…' : 'Apply'}
                        </button>
                    </div>
                </>
            )}
        </div>
    );
}
