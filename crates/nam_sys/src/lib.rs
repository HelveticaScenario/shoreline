//! Safe bindings to NeuralAmpModelerCore, the reference Neural Amp Modeler
//! runtime.
//!
//! [`NamFile`] is a parsed `.nam` file. [`NamDsp`] is one running model built
//! from it: building one allocates and prewarms, so it belongs on the main
//! thread; [`NamDsp::process`] never allocates and is safe on the audio
//! thread.

use std::ffi::{CStr, c_char, c_int};
use std::path::Path;
use std::ptr::NonNull;

#[repr(C)]
struct RawFile {
    _private: [u8; 0],
}

#[repr(C)]
struct RawDsp {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn nam_file_open(
        path: *const c_char,
        len: usize,
        err: *mut c_char,
        err_len: usize,
    ) -> *mut RawFile;
    fn nam_file_free(file: *mut RawFile);
    fn nam_dsp_new(
        file: *const RawFile,
        sample_rate: f64,
        max_frames: c_int,
        err: *mut c_char,
        err_len: usize,
    ) -> *mut RawDsp;
    fn nam_dsp_free(dsp: *mut RawDsp);
    fn nam_dsp_process(
        dsp: *mut RawDsp,
        input: *const f32,
        output: *mut f32,
        frames: c_int,
    ) -> c_int;
    fn nam_dsp_set_size(dsp: *mut RawDsp, size: f64) -> c_int;
    fn nam_dsp_loudness(dsp: *mut RawDsp, value: *mut f64) -> c_int;
}

const ERROR_LEN: usize = 512;

fn error_text(buf: &[c_char; ERROR_LEN]) -> String {
    // SAFETY: the shim always writes a NUL-terminated string within the buffer,
    // and the buffer starts zeroed.
    unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

/// A parsed `.nam` file.
pub struct NamFile {
    raw: NonNull<RawFile>,
}

// SAFETY: a parsed file is never mutated after `open`; models only read it.
unsafe impl Send for NamFile {}
// SAFETY: as above, every access through `&NamFile` is a read.
unsafe impl Sync for NamFile {}

impl NamFile {
    /// Reads and validates a `.nam` file, and checks its version is one the
    /// runtime supports.
    pub fn open(path: &Path) -> Result<Self, String> {
        let path = path
            .to_str()
            .ok_or_else(|| format!("{} is not a UTF-8 path", path.display()))?;
        let mut err = [0 as c_char; ERROR_LEN];
        // SAFETY: `path` is valid for `len` bytes and `err` for `ERROR_LEN`.
        let raw = unsafe {
            nam_file_open(
                path.as_ptr().cast(),
                path.len(),
                err.as_mut_ptr(),
                ERROR_LEN,
            )
        };
        NonNull::new(raw)
            .map(|raw| Self { raw })
            .ok_or_else(|| error_text(&err))
    }
}

impl Drop for NamFile {
    fn drop(&mut self) {
        // SAFETY: `raw` came from `nam_file_open` and is freed once.
        unsafe { nam_file_free(self.raw.as_ptr()) }
    }
}

/// One running mono model.
pub struct NamDsp {
    raw: NonNull<RawDsp>,
    max_frames: usize,
}

// SAFETY: a model holds no thread-affine state; it is used from one thread
// at a time, which `&mut self` on `process` enforces.
unsafe impl Send for NamDsp {}

impl NamDsp {
    /// Builds a model from `file` running at `sample_rate`, sized for calls
    /// of up to `max_frames` samples, and prewarms it. Allocates.
    pub fn new(file: &NamFile, sample_rate: f64, max_frames: usize) -> Result<Self, String> {
        let frames =
            c_int::try_from(max_frames.max(1)).map_err(|_| "block too large".to_string())?;
        let mut err = [0 as c_char; ERROR_LEN];
        // SAFETY: `file` is live and `err` is valid for `ERROR_LEN`.
        let raw = unsafe {
            nam_dsp_new(
                file.raw.as_ptr(),
                sample_rate,
                frames,
                err.as_mut_ptr(),
                ERROR_LEN,
            )
        };
        NonNull::new(raw)
            .map(|raw| Self {
                raw,
                max_frames: frames as usize,
            })
            .ok_or_else(|| error_text(&err))
    }

    /// Runs `input` through the model into `output`, in chunks of at most
    /// the `max_frames` it was built with. Never allocates. Returns `false`,
    /// with `output` partly written, if the model failed.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) -> bool {
        let len = input.len().min(output.len());
        let mut start = 0;
        while start < len {
            let end = (start + self.max_frames).min(len);
            // SAFETY: both slices are valid for `end - start` samples, at most
            // `max_frames`, which the model was sized for.
            let status = unsafe {
                nam_dsp_process(
                    self.raw.as_ptr(),
                    input[start..end].as_ptr(),
                    output[start..end].as_mut_ptr(),
                    (end - start) as c_int,
                )
            };
            if status != 0 {
                return false;
            }
            start = end;
        }
        true
    }

    /// Picks the tier to run of a model that holds several, from 0 (the
    /// smallest) to 1 (the largest, which runs by default), and prewarms it.
    /// Returns `false` for a model with one tier. Allocates.
    pub fn set_size(&mut self, size: f64) -> bool {
        // SAFETY: `raw` is live and `&mut self` keeps any `process` call out.
        unsafe { nam_dsp_set_size(self.raw.as_ptr(), size.clamp(0.0, 1.0)) != 0 }
    }

    /// How loud the model is for a typical input, in dB, if the file says.
    pub fn loudness(&self) -> Option<f64> {
        let mut value = 0.0;
        // SAFETY: `raw` is live; the shim only reads the model.
        (unsafe { nam_dsp_loudness(self.raw.as_ptr(), &mut value) } != 0).then_some(value)
    }
}

impl Drop for NamDsp {
    fn drop(&mut self) {
        // SAFETY: `raw` came from `nam_dsp_new` and is freed once.
        unsafe { nam_dsp_free(self.raw.as_ptr()) }
    }
}

/// Counts heap allocations and frees made anywhere in the process, C++ and
/// Rust alike, while `f` runs. Single-threaded tests only.
#[cfg(feature = "alloc-tracking")]
pub fn count_allocations(f: impl FnOnce()) -> i32 {
    unsafe extern "C" {
        fn nam_alloc_tracking_begin();
        fn nam_alloc_tracking_end() -> c_int;
    }
    // SAFETY: the tracker only flips a flag and reads two counters.
    unsafe { nam_alloc_tracking_begin() };
    f();
    // SAFETY: as above.
    unsafe { nam_alloc_tracking_end() }
}
