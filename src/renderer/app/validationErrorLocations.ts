import type { ValidationError } from '@modular/core';
import type { SourceLocationInfo } from '../../shared/ipcTypes';

/**
 * Point each validation error that names its module (`moduleId`) at that
 * module's source line. An auto-generated id means nothing to the user, so
 * the line replaces it; an explicit id stays beside it (`'myOsc' line 4`).
 */
export function transformErrorsWithSourceLocations(
    errors: ValidationError[],
    sourceLocationMap?: Record<string, SourceLocationInfo>,
): ValidationError[] {
    return errors.map((err) => {
        const own =
            err.moduleId === undefined
                ? undefined
                : sourceLocationMap?.[err.moduleId];
        if (!own) {
            return err;
        }
        return {
            ...err,
            location: own.idIsExplicit
                ? `'${err.moduleId}' line ${own.line}`
                : `line ${own.line}`,
        };
    });
}
