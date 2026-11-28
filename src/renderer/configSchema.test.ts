import { describe, expect, it } from 'vitest';
import { configSchema } from './configSchema';

describe('configSchema', () => {
    it('rejects unknown top-level and audioConfig keys', () => {
        expect(configSchema.additionalProperties).toBe(false);
        expect(configSchema.properties?.audioConfig).toMatchObject({
            additionalProperties: false,
        });
    });

    it('describes every field', () => {
        const missing: string[] = [];
        const walk = (schema: Record<string, unknown>, path: string) => {
            const properties = schema.properties as
                Record<string, Record<string, unknown>> | undefined;
            for (const [key, field] of Object.entries(properties ?? {})) {
                if (typeof field.description !== 'string') {
                    missing.push(`${path}${key}`);
                }
                walk(field, `${path}${key}.`);
            }
        };
        walk(configSchema as Record<string, unknown>, '');
        expect(missing).toEqual([]);
    });
});
