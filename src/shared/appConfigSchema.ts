import { z } from 'zod';
import { CURSOR_STYLES, DEFAULT_CURSOR_STYLE } from './cursorStyle';
import { MONOSPACE_FONTS } from './fonts';

/** Ids of the themes the app ships with. */
const BUNDLED_THEME_IDS = [
    'modular-dark',
    'one-dark-pro',
    'dracula',
    'gruvbox-dark',
    'tokyo-night',
    'catppuccin-mocha',
];

const MonospaceFontSchema = z.enum(MONOSPACE_FONTS);

/**
 * A theme id parses as any string, so a config naming a theme this build
 * lacks keeps it on disk; the editor's JSON Schema still lists the bundled
 * ones for completion and warnings.
 */
const ThemeSchema = z.string().meta({ enum: BUNDLED_THEME_IDS });

const AudioConfigSchema = z
    .object({
        hostId: z.string().optional().meta({
            description: "Audio host identifier (e.g., 'CoreAudio', 'WASAPI')",
        }),
        inputDeviceId: z.string().nullable().optional().meta({
            description: 'Input device ID, or null for no input',
        }),
        outputDeviceId: z.string().optional().meta({
            description:
                'Output device ID; omit to follow the system default output',
        }),
        sampleRate: z.number().optional().meta({
            description: 'Sample rate in Hz (e.g., 44100, 48000)',
        }),
        bufferSize: z.number().optional().meta({
            description: 'Buffer size in samples (e.g., 256, 512)',
        }),
    })
    .meta({ description: 'Audio device configuration' });

/** How Format Document quotes strings. `backtick` prints each string as a
 *  template literal wherever the syntax allows one. */
const QuoteStyleSchema = z.enum(['single', 'double', 'backtick']);

/** Unlisted keys pass through to Prettier as they are. */
const PrettierConfigSchema = z
    .looseObject({
        quotes: QuoteStyleSchema.optional().meta({
            default: 'single',
            description:
                'Quotes for strings; backtick keeps quotes where JavaScript requires them, such as import paths and object keys',
        }),
        trailingComma: z
            .enum(['all', 'es5', 'none'])
            .optional()
            .meta({ default: 'all', description: 'Trailing comma style' }),
        semi: z.boolean().optional().meta({
            default: false,
            description: 'Add semicolons at the end of statements',
        }),
        tabWidth: z.number().optional().meta({
            default: 2,
            description: 'Number of spaces per indentation level',
        }),
        printWidth: z.number().optional().meta({
            default: 60,
            description: 'Line width before wrapping',
        }),
    })
    .meta({
        description:
            "Prettier formatting options. Merged with defaults: quotes='single', trailingComma='all', semi=false, tabWidth=2, printWidth=60. The parser and plugins cannot be overridden.",
    });

/**
 * The shape of config.json. Main parses the file with it and the editor
 * validates the file against its JSON Schema, so every field's `meta` is what
 * the editor shows on hover.
 */
export const AppConfigSchema = z
    .object({
        audioConfig: AudioConfigSchema.optional(),
        cursorStyle: z.enum(CURSOR_STYLES).optional().meta({
            default: DEFAULT_CURSOR_STYLE,
            description: 'The cursor style in the editor',
        }),
        font: MonospaceFontSchema.optional().meta({
            default: 'Fira Code',
            description: 'The monospace font family used in the editor',
        }),
        fontLigatures: z.boolean().optional().meta({
            default: false,
            description: 'Enable or disable font ligatures in the editor',
        }),
        fontSize: z.number().min(8).max(72).optional().meta({
            default: 17,
            description: 'The font size in the editor (8–72)',
        }),
        lastOpenedFolder: z.string().optional().meta({
            description:
                'Path to the last opened workspace folder (managed automatically)',
        }),
        lastSeenVersion: z.string().optional().meta({
            description:
                'App version whose release notes were last shown (managed automatically)',
        }),
        prettier: PrettierConfigSchema.optional(),
        skippedUpdateVersion: z.string().optional().meta({
            description:
                'Release version whose update prompt was dismissed (managed automatically)',
        }),
        theme: ThemeSchema.optional().meta({
            default: 'modular-dark',
            description: 'The color theme for the application and editor',
        }),
        codeBackdropOpacity: z.number().min(0).max(1).optional().meta({
            default: 0.6,
            description:
                'Opacity of the backing behind the code while video plays (0–1)',
        }),
        performanceAspect: z
            .enum(['free', '16:9', '4:3', '1:1', '9:16'])
            .optional()
            .meta({
                default: 'free',
                description:
                    'Shape of the performance window; free lets it take any size',
            }),
        performanceScale: z.number().min(0.1).max(1).optional().meta({
            default: 1,
            description:
                'Resolution of the performance window as a fraction of its size (0.1–1)',
        }),
        performanceCodeVisible: z.boolean().optional().meta({
            default: true,
            description:
                'Show the code in the performance window; View, Toggle Performance Code fades it in and out',
        }),
        performanceCodeOpacity: z.number().min(0).max(1).optional().meta({
            default: 1,
            description:
                'Opacity of the code in the performance window while it shows (0–1)',
        }),
        performanceCodeBacking: z.number().min(0).max(1).optional().meta({
            default: 0.6,
            description:
                'Opacity of the backing behind each line of code in the performance window (0–1)',
        }),
        performanceFont: MonospaceFontSchema.optional().meta({
            default: 'Fira Code',
            description:
                'The monospace font family of the code in the performance window',
        }),
        performanceFontSize: z.number().min(8).max(160).optional().meta({
            default: 28,
            description:
                'The font size of the code in the performance window (8–160)',
        }),
        performanceLineHeight: z.number().min(1).max(3).optional().meta({
            default: 1.4,
            description:
                'Line height of the code in the performance window, as a multiple of its font size (1–3)',
        }),
        performanceFontLigatures: z.boolean().optional().meta({
            default: false,
            description: 'Enable font ligatures in the performance window',
        }),
        performanceLineNumbers: z.boolean().optional().meta({
            default: false,
            description: 'Show line numbers in the performance window',
        }),
        performanceScrollSpeed: z.number().min(0).max(1).optional().meta({
            default: 0.5,
            description:
                'How quickly the code in the performance window scrolls to follow the editor (0–1); 1 jumps',
        }),
        performanceTheme: ThemeSchema.optional().meta({
            description:
                "The color theme of the code in the performance window; absent follows the editor's theme",
        }),
        sidebarSide: z.enum(['left', 'right']).optional().meta({
            default: 'right',
            description: 'Which side of the window the sidebar sits on',
        }),
        bottomPanel: z.enum(['mixer', 'docs']).optional().meta({
            description:
                'The panel open under the editor; leave unset to collapse it',
        }),
        docsLocation: z.enum(['bottom', 'sidebar']).optional().meta({
            default: 'bottom',
            description:
                'Where the docs open: a panel under the editor or a sidebar tab',
        }),
        bottomPanelHeight: z.number().min(84).optional().meta({
            default: 200,
            description: 'Height of the panel under the editor in pixels',
        }),
    })
    .meta({
        title: 'Shoreline Configuration',
        description:
            'Configuration file for the Shoreline synthesizer application',
    });

export type AppConfig = z.infer<typeof AppConfigSchema>;
export type PrettierConfig = z.infer<typeof PrettierConfigSchema>;
export type QuoteStyle = z.infer<typeof QuoteStyleSchema>;
