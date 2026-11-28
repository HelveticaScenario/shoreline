import type { Uri } from 'monaco-editor';
import type { Monaco } from '../../hooks/useCustomMonaco';
import type { DslTypeScriptWorkerApi } from './dslTsWorker';

/** The TypeScript worker serving `resource`, with the DSL's own requests. */
export async function dslWorker(monaco: Monaco, resource: Uri) {
    const worker = await (
        await monaco.typescript.getJavaScriptWorker()
    )(resource);
    return worker as typeof worker & DslTypeScriptWorkerApi;
}
