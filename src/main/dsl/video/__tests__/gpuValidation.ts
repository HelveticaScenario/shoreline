/// <reference types="@webgpu/types" />
import { create, globals } from 'webgpu';
import type { CompiledVideoShader } from '../../../../shared/video/videoGraph';
import { ShaderProgram } from '../../../../renderer/video/ShaderProgram';
import { requestVideoDevice } from '../../../../renderer/video/videoDevice';

Object.assign(globalThis, globals);

/** Held for the life of the process: Dawn tears down its devices with it. */
const dawn = create([]);
let device: Promise<GPUDevice> | undefined;

/**
 * One Dawn device shared by every test, the WebGPU implementation Electron
 * uses. Headless Linux needs a software Vulkan driver such as Mesa's lavapipe.
 */
function gpuDevice(): Promise<GPUDevice> {
    device ??= (async () => {
        const adapter = await dawn.requestAdapter();
        if (!adapter)
            throw new Error('no WebGPU adapter: install a Vulkan driver');
        const created = await requestVideoDevice(adapter);
        void created.lost.then(() => {
            device = undefined;
        });
        return created;
    })();
    return device;
}

/**
 * The errors WebGPU reports building `compiled` the way the renderer does:
 * the WGSL compiler's messages or, when the WGSL compiles, any validation,
 * internal or out-of-memory error from the bind group layout and the
 * pipelines. Empty when the renderer would accept
 * the shader.
 */
export async function shaderErrors(
    compiled: CompiledVideoShader,
): Promise<string[]> {
    const gpu = await gpuDevice();
    const scopes: GPUErrorFilter[] = [
        'validation',
        'internal',
        'out-of-memory',
    ];
    for (const filter of scopes) gpu.pushErrorScope(filter);
    const built = await ShaderProgram.build(gpu, 'bgra8unorm', compiled).then(
        () => null,
        (error: unknown) =>
            error instanceof Error ? error.message : String(error),
    );
    const scoped = [];
    for (let k = scopes.length; k > 0; k--) {
        const error = await gpu.popErrorScope();
        if (error) scoped.push(error.message);
    }
    return built !== null ? [built] : scoped;
}
