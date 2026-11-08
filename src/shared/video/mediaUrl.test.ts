import { describe, expect, it } from 'vitest';
import {
    isRemoteMedia,
    mediaUrl,
    parseMediaUrl,
    parseRemoteMediaUrl,
} from './mediaUrl';

describe('mediaUrl', () => {
    it('round-trips a path, including characters that need escaping', () => {
        for (const path of [
            'photo.png',
            'pictures/photo.png',
            'my clips/loop #2 (final).mp4',
            'ünïcode/ファイル.webp',
            'a%20b/c.png',
        ]) {
            expect(parseMediaUrl(mediaUrl(path))).toBe(path);
        }
    });

    it('escapes each segment but keeps the separators', () => {
        expect(mediaUrl('a b/c d.png')).toBe(
            'shoreline-media://workspace/a%20b/c%20d.png',
        );
    });

    it('rejects a URL of another scheme or host', () => {
        expect(parseMediaUrl('https://workspace/a.png')).toBeNull();
        expect(parseMediaUrl('shoreline-media://elsewhere/a.png')).toBeNull();
        expect(parseMediaUrl('file:///etc/passwd')).toBeNull();
        expect(parseMediaUrl('not a url')).toBeNull();
    });

    it('rejects broken escapes', () => {
        expect(
            parseMediaUrl('shoreline-media://workspace/%E0%A4%A.png'),
        ).toBeNull();
    });

    it('has parent segments collapsed by the URL parser, even when escaped', () => {
        expect(
            parseMediaUrl('shoreline-media://workspace/a/%2e%2e/b.png'),
        ).toBe('b.png');
        expect(parseMediaUrl(mediaUrl('../../x.png'))).toBe('x.png');
    });

    describe('network URLs', () => {
        const address =
            'https://cdn.example.com/pictures/photo.jpg?token=a b&x=1#t';

        it('tells a network URL from a workspace path', () => {
            expect(isRemoteMedia('http://a/b.mp4')).toBe(true);
            expect(isRemoteMedia('HTTPS://a/b.mp4')).toBe(true);
            expect(isRemoteMedia('clips/a.mp4')).toBe(false);
            expect(isRemoteMedia('rtsp://a/b')).toBe(false);
            expect(isRemoteMedia('file:///a.mp4')).toBe(false);
        });

        it('round-trips a network URL with a query and a fragment', () => {
            expect(parseRemoteMediaUrl(mediaUrl(address))).toBe(address);
        });

        it('keeps a network URL out of the workspace', () => {
            expect(parseMediaUrl(mediaUrl(address))).toBeNull();
            expect(parseRemoteMediaUrl(mediaUrl('clips/a.mp4'))).toBeNull();
        });

        it('rejects what is not a network URL', () => {
            expect(parseRemoteMediaUrl('https://cdn.example.com/a')).toBeNull();
            expect(
                parseRemoteMediaUrl(
                    `shoreline-media://remote/${encodeURIComponent('file:///etc/passwd')}`,
                ),
            ).toBeNull();
            expect(
                parseRemoteMediaUrl('shoreline-media://remote/%E0%A4%A'),
            ).toBeNull();
        });
    });
});
