import { useState } from 'react';
import type { ValidationError } from '@modular/core';
import type { DSLExecuteResult } from '../../../shared/ipcTypes';
import { transformErrorsWithSourceLocations } from '../validationErrorLocations';

export interface PatchErrors {
    error: string | null;
    validationErrors: ValidationError[] | null;
    /** Show an error message, keeping any validation errors listed. */
    show: (message: string) => void;
    /** Show an error message in place of everything else. */
    fail: (message: string) => void;
    /** Show why a patch run failed, with each validation error at its
     *  source line. */
    showFailure: (result: DSLExecuteResult) => void;
    clear: () => void;
}

/** The error banner's state: one message, and a failed patch's validation
 *  errors when it has them. */
export function usePatchErrors(): PatchErrors {
    const [state, setState] = useState<{
        error: string | null;
        validationErrors: ValidationError[] | null;
    }>({ error: null, validationErrors: null });

    const show = (message: string) =>
        setState((prev) => ({ ...prev, error: message }));

    const fail = (message: string) =>
        setState({ error: message, validationErrors: null });

    const showFailure = (result: DSLExecuteResult) => {
        if (result.errorMessage) {
            fail(result.errorMessage);
        } else if (result.errors && result.errors.length > 0) {
            setState({
                error:
                    result.errors.map((e) => e.message).join('\n') ||
                    'Failed to apply patch.',
                validationErrors: transformErrorsWithSourceLocations(
                    result.errors.flatMap((e) => e.errors || []),
                    result.sourceLocationMap,
                ),
            });
        }
    };

    const clear = () => setState({ error: null, validationErrors: null });

    return { ...state, clear, fail, show, showFailure };
}
