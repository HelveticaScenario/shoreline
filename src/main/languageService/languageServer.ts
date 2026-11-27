/**
 * Hosts the TypeScript 7 language server for patch buffers. The server is a
 * native process speaking LSP over stdio; the renderer's editor reaches it
 * through IPC. Buffers are virtual `.sl` documents inside a project directory
 * that holds only the DSL's declarations and the content mapper that reads
 * track labels (see slContentMapper).
 */
import { type ChildProcess, spawn } from 'node:child_process';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { pathToFileURL } from 'node:url';
import { app } from 'electron';
import { JsonRpcConnection } from './jsonRpc';

const MAPPER_PACKAGE = 'shoreline-content-mapper';

export interface LanguageServerInfo {
    /** `file://` URI of the directory virtual buffers live in. */
    buffersUri: string;
    /** `file://` URI of the DSL's declarations. */
    libUri: string;
}

interface Running {
    process: ChildProcess;
    connection: JsonRpcConnection;
    info: LanguageServerInfo;
    ready: Promise<void>;
}

let running: Running | null = null;

function serverExecutable(): string {
    const name = process.platform === 'win32' ? 'tsc.exe' : 'tsc';
    const relative = path.join(
        'node_modules',
        '@typescript',
        `typescript-${process.platform}-${process.arch}`,
        'lib',
        name,
    );
    // The app's directory is the build output in development and the
    // package root once packaged; the server sits in an ancestor's modules.
    for (let dir = __dirname; ; dir = path.dirname(dir)) {
        const candidate = path.join(dir, relative);
        if (fs.existsSync(candidate)) {
            return candidate;
        }
        if (dir === path.dirname(dir)) {
            throw new Error(
                `TypeScript language server not found: ${relative}`,
            );
        }
    }
}

/** Writes the project the server reads: config, declarations and mapper. */
function writeProject(root: string, libSource: string): void {
    const mapperDir = path.join(root, 'node_modules', MAPPER_PACKAGE);
    fs.mkdirSync(mapperDir, { recursive: true });
    fs.mkdirSync(path.join(root, 'buffers'), { recursive: true });
    fs.writeFileSync(path.join(root, 'dsl-lib.d.ts'), libSource);
    fs.writeFileSync(
        path.join(root, 'tsconfig.json'),
        JSON.stringify(
            {
                compilerOptions: {
                    allowJs: true,
                    allowUnusedLabels: true,
                    checkJs: true,
                    lib: ['esnext'],
                    module: 'esnext',
                    moduleResolution: 'bundler',
                    noEmit: true,
                    skipLibCheck: true,
                    strict: false,
                    target: 'es2020',
                    types: [],
                },
                contentMappers: [
                    { extensions: ['.sl'], package: MAPPER_PACKAGE },
                ],
                include: ['dsl-lib.d.ts', 'buffers/*.sl'],
            },
            null,
            2,
        ),
    );
    fs.writeFileSync(
        path.join(mapperDir, 'package.json'),
        JSON.stringify({
            name: MAPPER_PACKAGE,
            typescript: {
                contentMapper: {
                    // Electron runs the mapper as plain Node: the server's
                    // environment, which the mapper inherits, sets
                    // ELECTRON_RUN_AS_NODE.
                    exec: [
                        process.execPath,
                        path.join(__dirname, 'slContentMapper.js'),
                    ],
                },
            },
            version: app.getVersion(),
        }),
    );
}

export function startLanguageServer(libSource: string): Running {
    if (running) {
        return running;
    }
    const dir = path.join(app.getPath('userData'), 'language-service');
    fs.mkdirSync(dir, { recursive: true });
    // The server names documents by their real path, so the URIs handed to
    // the renderer use it too.
    const root = fs.realpathSync(dir);
    writeProject(root, libSource);

    const child = spawn(serverExecutable(), ['--lsp', '--stdio'], {
        cwd: root,
        env: { ...process.env, ELECTRON_RUN_AS_NODE: '1' },
        stdio: ['pipe', 'pipe', 'pipe'],
    });
    child.stderr.on('data', (data: Buffer) => {
        console.warn('[language-server]', data.toString().trimEnd());
    });
    child.on('error', (error) => {
        console.warn('[language-server] failed to start:', error.message);
    });
    child.on('exit', (code) => {
        console.warn('[language-server] exited with code', code);
        running = null;
    });

    const connection = new JsonRpcConnection(child.stdout, child.stdin);
    connection.onRequest((method, params) => {
        if (method === 'workspace/configuration') {
            return (params as { items: unknown[] }).items.map(() => null);
        }
        return null;
    });

    const rootUri = pathToFileURL(root).toString();
    const ready = connection
        .sendRequest('initialize', {
            capabilities: {
                textDocument: {
                    codeAction: {
                        codeActionLiteralSupport: {
                            codeActionKind: {
                                valueSet: ['quickfix', 'refactor', 'source'],
                            },
                        },
                        resolveSupport: { properties: ['edit'] },
                    },
                    completion: {
                        completionItem: {
                            documentationFormat: ['markdown'],
                            labelDetailsSupport: true,
                            resolveSupport: {
                                properties: ['documentation', 'detail'],
                            },
                        },
                    },
                    diagnostic: {},
                    documentSymbol: {
                        hierarchicalDocumentSymbolSupport: true,
                    },
                    hover: { contentFormat: ['markdown'] },
                    rename: { prepareSupport: true },
                    signatureHelp: {
                        signatureInformation: {
                            activeParameterSupport: true,
                            documentationFormat: ['markdown'],
                            parameterInformation: {
                                labelOffsetSupport: true,
                            },
                        },
                    },
                },
                workspace: { configuration: true },
            },
            // Lets the server run the content mapper this project names.
            initializationOptions: { runExternalCode: true },
            processId: process.pid,
            rootUri,
        })
        .then(() => {
            connection.sendNotification('initialized', {});
        });

    running = {
        connection,
        info: {
            buffersUri: `${rootUri}/buffers`,
            libUri: `${rootUri}/dsl-lib.d.ts`,
        },
        process: child,
        ready,
    };
    app.on('will-quit', () => child.kill());
    return running;
}

export async function languageServerRequest(
    server: Running,
    method: string,
    params: unknown,
): Promise<unknown> {
    await server.ready;
    return server.connection.sendRequest(method, params);
}

export async function languageServerNotify(
    server: Running,
    method: string,
    params: unknown,
): Promise<void> {
    await server.ready;
    server.connection.sendNotification(method, params);
}
