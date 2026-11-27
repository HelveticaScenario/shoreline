import type { Readable, Writable } from 'node:stream';

type Json = unknown;

interface Pending {
    resolve: (value: Json) => void;
    reject: (error: Error) => void;
}

/**
 * JSON-RPC 2.0 over a pair of streams with `Content-Length` framing, the
 * transport both the TypeScript language server and its content mappers
 * speak.
 */
export class JsonRpcConnection {
    private buffer = Buffer.alloc(0);
    private nextId = 1;
    private readonly pending = new Map<number | string, Pending>();
    private requestHandler: (method: string, params: Json) => Json = () => null;
    private notificationHandler: (method: string, params: Json) => void =
        () => {};

    constructor(
        input: Readable,
        private readonly output: Writable,
    ) {
        input.on('data', (chunk: Buffer) => {
            this.buffer = Buffer.concat([this.buffer, chunk]);
            this.drain();
        });
        input.on('close', () => this.failAll('connection closed'));
    }

    onRequest(handler: (method: string, params: Json) => Json): void {
        this.requestHandler = handler;
    }

    onNotification(handler: (method: string, params: Json) => void): void {
        this.notificationHandler = handler;
    }

    sendRequest<T = Json>(method: string, params?: Json): Promise<T> {
        const id = this.nextId++;
        return new Promise<T>((resolve, reject) => {
            this.pending.set(id, {
                reject,
                resolve: resolve as (value: Json) => void,
            });
            this.write({ id, jsonrpc: '2.0', method, params });
        });
    }

    sendNotification(method: string, params?: Json): void {
        this.write({ jsonrpc: '2.0', method, params });
    }

    private failAll(reason: string): void {
        for (const { reject } of this.pending.values()) {
            reject(new Error(reason));
        }
        this.pending.clear();
    }

    private write(message: Json): void {
        const body = Buffer.from(JSON.stringify(message), 'utf8');
        this.output.write(`Content-Length: ${body.length}\r\n\r\n`);
        this.output.write(body);
    }

    private drain(): void {
        for (;;) {
            const headerEnd = this.buffer.indexOf('\r\n\r\n');
            if (headerEnd === -1) {
                return;
            }
            const header = this.buffer.subarray(0, headerEnd).toString('ascii');
            const match = /Content-Length: *(\d+)/i.exec(header);
            if (!match) {
                this.buffer = this.buffer.subarray(headerEnd + 4);
                continue;
            }
            const length = Number(match[1]);
            const bodyStart = headerEnd + 4;
            if (this.buffer.length < bodyStart + length) {
                return;
            }
            const body = this.buffer
                .subarray(bodyStart, bodyStart + length)
                .toString('utf8');
            this.buffer = this.buffer.subarray(bodyStart + length);
            this.dispatch(JSON.parse(body));
        }
    }

    private dispatch(message: {
        id?: number | string;
        method?: string;
        params?: Json;
        result?: Json;
        error?: { message: string };
    }): void {
        if (message.method === undefined) {
            const pending = this.pending.get(message.id!);
            if (!pending) {
                return;
            }
            this.pending.delete(message.id!);
            if (message.error) {
                pending.reject(new Error(message.error.message));
            } else {
                pending.resolve(message.result);
            }
            return;
        }
        if (message.id === undefined) {
            this.notificationHandler(message.method, message.params);
            return;
        }
        const { id, method, params } = message;
        Promise.resolve()
            .then(() => this.requestHandler(method, params))
            .then(
                (result) =>
                    this.write({ id, jsonrpc: '2.0', result: result ?? null }),
                (error: unknown) =>
                    this.write({
                        error: {
                            code: -32603,
                            message:
                                error instanceof Error
                                    ? error.message
                                    : String(error),
                        },
                        id,
                        jsonrpc: '2.0',
                    }),
            );
    }
}
