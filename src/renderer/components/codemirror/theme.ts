import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import type { Extension } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { type Tag, tags as t } from '@lezer/highlight';
import type { AppTheme, VSCodeTokenColor } from '../../themes/types';

/**
 * The TextMate scopes that color each syntax tag, most specific first. An app
 * theme is a VS Code theme, whose colors are keyed by TextMate scope; the
 * editor highlights by tag.
 */
const TAG_SCOPES: [Tag | readonly Tag[], string[]][] = [
    [t.comment, ['comment']],
    [[t.lineComment, t.blockComment, t.docComment], ['comment']],
    [t.string, ['string']],
    [t.special(t.string), ['string.template', 'string']],
    [t.regexp, ['string.regexp', 'string']],
    [t.escape, ['constant.character.escape', 'constant.character']],
    [t.number, ['constant.numeric', 'constant']],
    [
        [t.bool, t.null, t.atom],
        ['constant.language', 'constant'],
    ],
    [t.keyword, ['keyword']],
    [t.controlKeyword, ['keyword.control', 'keyword']],
    [t.operatorKeyword, ['keyword.operator.expression', 'keyword']],
    [t.definitionKeyword, ['storage.type', 'storage', 'keyword']],
    [t.modifier, ['storage.modifier', 'storage', 'keyword']],
    [t.moduleKeyword, ['keyword.control.import', 'keyword.control', 'keyword']],
    [t.self, ['variable.language']],
    [
        [t.operator, t.arithmeticOperator, t.logicOperator, t.compareOperator],
        ['keyword.operator'],
    ],
    [t.variableName, ['variable.other.readwrite', 'variable']],
    [t.definition(t.variableName), ['variable.other.constant', 'variable']],
    [t.local(t.variableName), ['variable.other.readwrite', 'variable']],
    [
        t.function(t.variableName),
        ['entity.name.function', 'support.function', 'meta.function-call'],
    ],
    [
        t.function(t.definition(t.variableName)),
        ['entity.name.function', 'support.function'],
    ],
    [
        t.function(t.propertyName),
        ['entity.name.function', 'support.function', 'meta.function-call'],
    ],
    [
        t.propertyName,
        ['variable.other.property', 'meta.object-literal.key', 'variable'],
    ],
    [
        t.definition(t.propertyName),
        ['meta.object-literal.key', 'variable.other.property', 'variable'],
    ],
    [
        [t.typeName, t.className, t.namespace],
        ['entity.name.type', 'support.type', 'support.class', 'entity.name'],
    ],
    [t.labelName, ['entity.name.label', 'entity.name.function']],
    [[t.punctuation, t.separator], ['punctuation']],
    [
        [t.bracket, t.paren, t.squareBracket, t.brace],
        ['punctuation.definition', 'punctuation', 'meta.brace'],
    ],
    [t.invalid, ['invalid']],
];

interface ScopeStyle {
    scope: string;
    settings: VSCodeTokenColor['settings'];
}

function scopeStyles(theme: AppTheme): ScopeStyle[] {
    return theme.raw.tokenColors.flatMap((entry) => {
        const scopes = Array.isArray(entry.scope)
            ? entry.scope
            : (entry.scope ?? '').split(',');
        return scopes
            .map((scope) => scope.trim())
            .filter((scope) => scope !== '')
            .map((scope) => ({ scope, settings: entry.settings }));
    });
}

/**
 * The settings a TextMate scope gets: those of the theme's longest rule that
 * names the scope or one of its ancestors, later rules winning ties.
 */
function settingsFor(
    styles: ScopeStyle[],
    scope: string,
): VSCodeTokenColor['settings'] | null {
    let best: ScopeStyle | null = null;
    for (const style of styles) {
        const selector = style.scope.split(/\s+/).pop()!;
        if (scope !== selector && !scope.startsWith(`${selector}.`)) {
            continue;
        }
        if (!best || selector.length >= best.scope.length) {
            best = { scope: selector, settings: style.settings };
        }
    }
    return best?.settings ?? null;
}

export function highlightStyleFor(theme: AppTheme): HighlightStyle {
    const styles = scopeStyles(theme);
    return HighlightStyle.define(
        TAG_SCOPES.flatMap(([tag, scopes]) => {
            for (const scope of scopes) {
                const settings = settingsFor(styles, scope);
                if (!settings?.foreground && !settings?.fontStyle) {
                    continue;
                }
                const fontStyle = settings.fontStyle ?? '';
                return [
                    {
                        color: settings.foreground,
                        fontStyle: fontStyle.includes('italic')
                            ? 'italic'
                            : undefined,
                        fontWeight: fontStyle.includes('bold')
                            ? 'bold'
                            : undefined,
                        tag,
                        textDecoration: fontStyle.includes('underline')
                            ? 'underline'
                            : undefined,
                    },
                ];
            }
            return [];
        }),
    );
}

interface Appearance {
    fontFamily: string;
    fontSize: number;
    fontLigatures: boolean;
}

/** The editor's chrome and syntax colors for an app theme. */
export function editorTheme(theme: AppTheme, look: Appearance): Extension {
    const c = theme.colors;
    const raw = theme.raw.colors;
    const widgetBackground = raw['editorWidget.background'] ?? c.bgSecondary;
    const widgetBorder = raw['editorWidget.border'] ?? c.borderDefault;
    return [
        EditorView.theme(
            {
                '&': {
                    // Transparent, so a video picture shows behind the code.
                    backgroundColor: 'transparent',
                    color: c.editorForeground,
                    fontSize: `${look.fontSize}px`,
                    height: '100%',
                },
                '&.cm-focused': { outline: 'none' },
                '.cm-scroller': {
                    fontFamily: look.fontFamily,
                    fontVariantLigatures: look.fontLigatures
                        ? 'normal'
                        : 'none',
                    lineHeight: '1.5',
                },
                '.cm-content': {
                    caretColor: c.editorCursor,
                    padding: '8px 0',
                },
                '.cm-cursor, .cm-dropCursor': {
                    borderLeftColor: c.editorCursor,
                    borderLeftWidth: '2px',
                },
                '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground':
                    { backgroundColor: c.editorSelection },
                '.cm-activeLine': {
                    backgroundColor: `color-mix(in srgb, ${c.editorLineHighlight} 40%, transparent)`,
                },
                '.cm-gutters': {
                    backgroundColor: 'transparent',
                    border: 'none',
                    color: c.lineNumberForeground,
                },
                '.cm-activeLineGutter': {
                    backgroundColor: 'transparent',
                    color: c.lineNumberActiveForeground,
                },
                '.cm-lineNumbers .cm-gutterElement': {
                    minWidth: '3ch',
                    padding: '0 12px 0 16px',
                },
                '.cm-matchingBracket, &.cm-focused .cm-matchingBracket': {
                    backgroundColor: 'transparent',
                    outline: `1px solid ${c.textMuted}`,
                },
                '.cm-selectionMatch': {
                    backgroundColor: `color-mix(in srgb, ${c.editorSelection} 55%, transparent)`,
                },
                '.cm-tooltip': {
                    backgroundColor: widgetBackground,
                    border: `1px solid ${widgetBorder}`,
                    borderRadius: '4px',
                    boxShadow: '0 4px 16px rgba(0, 0, 0, 0.5)',
                    color: c.textPrimary,
                },
                '.cm-tooltip-autocomplete > ul': {
                    fontFamily: look.fontFamily,
                    maxHeight: '18em',
                },
                '.cm-tooltip-autocomplete > ul > li': { padding: '2px 8px' },
                '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
                    backgroundColor:
                        raw['editorSuggestWidget.selectedBackground'] ??
                        c.bgActive,
                    color: c.textBright,
                },
                '.cm-completionDetail': {
                    color: c.textMuted,
                    fontStyle: 'normal',
                    marginLeft: '1.5em',
                },
                '.cm-completionInfo': {
                    maxWidth: '520px',
                    padding: '6px 10px',
                },
                '.cm-panels': {
                    backgroundColor: c.bgSecondary,
                    color: c.textPrimary,
                },
                '.cm-diagnostic': { fontFamily: look.fontFamily },
                '.cm-lintRange-hint': {
                    backgroundImage: 'none',
                    opacity: '0.55',
                },
            },
            { dark: theme.type === 'dark' },
        ),
        syntaxHighlighting(highlightStyleFor(theme)),
    ];
}
