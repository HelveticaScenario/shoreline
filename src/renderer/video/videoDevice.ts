/// <reference types="@webgpu/types" />
import { MAX_FEEDBACK_BUFFERS } from '../../shared/video/videoGraph';

/**
 * Bytes per pixel the shader writes when every frame buffer is in use: each
 * is a half-float color of 8 bytes, and the canvas takes 8 more once its
 * format is padded. The default limit is 32, which holds only three buffers.
 */
const COLOR_BYTES_PER_SAMPLE = 8 + MAX_FEEDBACK_BUFFERS * 8;

/** A device with the limits the video shaders need, as far as `adapter` allows. */
export function requestVideoDevice(adapter: GPUAdapter): Promise<GPUDevice> {
    return adapter.requestDevice({
        requiredLimits: {
            maxColorAttachmentBytesPerSample: Math.min(
                adapter.limits.maxColorAttachmentBytesPerSample,
                COLOR_BYTES_PER_SAMPLE,
            ),
        },
    });
}
