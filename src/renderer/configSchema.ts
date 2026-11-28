import { z } from 'zod';
import { AppConfigSchema } from '../shared/appConfigSchema';

/** The JSON Schema Monaco validates config.json against and completes it from. */
export const configSchema = z.toJSONSchema(AppConfigSchema, {
    target: 'draft-7',
});
