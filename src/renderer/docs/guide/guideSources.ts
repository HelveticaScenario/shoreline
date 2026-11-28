/** Each chapter's MDX source, keyed by chapter id, for the tests that read it. */
export const GUIDE_SOURCES: Readonly<Record<string, string>> =
    Object.fromEntries(
        Object.entries(
            import.meta.glob<string>('./*.mdx', {
                eager: true,
                import: 'default',
                query: '?raw',
            }),
        ).map(([path, source]) => [path.replace(/^\.\/|\.mdx$/g, ''), source]),
    );
