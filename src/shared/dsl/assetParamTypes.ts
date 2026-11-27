/**
 * The schema `$defs` names of params a patch writes as a file path, which the
 * DSL resolves to the value the engine reads. Each has an entry in
 * `ASSET_PARAM_TYPES` (`src/main/dsl/assetParams.ts`).
 */
export const ASSET_PARAM_TYPE_NAMES = ['Wav', 'Dx7Voice'] as const;

export type AssetParamTypeName = (typeof ASSET_PARAM_TYPE_NAMES)[number];

export function isAssetParamTypeName(name: string): name is AssetParamTypeName {
    return (ASSET_PARAM_TYPE_NAMES as readonly string[]).includes(name);
}
