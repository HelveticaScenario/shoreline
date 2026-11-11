/** The kinds of workspace file a patch can load. */
export type AssetKind = 'wav' | 'image' | 'video';

/** File extensions of each asset kind, lower case and without the dot. */
export const ASSET_EXTENSIONS: Record<AssetKind, readonly string[]> = {
    wav: ['wav'],
    image: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'avif'],
    video: ['mp4', 'webm', 'mov', 'm4v', 'ogv'],
};

/** The asset kind a file name's extension marks, or undefined for any other file. */
export function assetKind(fileName: string): AssetKind | undefined {
    const dot = fileName.lastIndexOf('.');
    if (dot === -1) return undefined;
    const extension = fileName.slice(dot + 1).toLowerCase();
    return (Object.keys(ASSET_EXTENSIONS) as AssetKind[]).find((kind) =>
        ASSET_EXTENSIONS[kind].includes(extension),
    );
}
