//! Scope and VU meter reads for the audio callback's drain.

use modular_core::types::{PortView, Sampleable, SampleableMap};

/// One lane of a module output port, resolved once per drained block so the
/// per-sample reads skip the id and port-name lookups.
pub(crate) enum Tap<'a> {
    /// A Block-mode port read straight from its output buffer.
    View {
        data: *const f32,
        stride: usize,
        offset: usize,
        processed: *const std::cell::Cell<usize>,
    },
    /// A port with no raw view (Sample-mode wrappers), read through the module.
    Module {
        module: &'a dyn Sampleable,
        port: &'a str,
        lane: usize,
    },
}

impl<'a> Tap<'a> {
    /// `None` when the module is not in the patch.
    ///
    /// The tap is only valid for reads below the slot every module has been
    /// processed to; the drain resolves taps after `process_all_modules_to`
    /// has advanced the whole patch through the block.
    pub(crate) fn resolve(
        sampleables: &'a SampleableMap,
        module_id: &str,
        port: &'a str,
        lane: usize,
    ) -> Option<Self> {
        let module = sampleables.get(module_id)?.as_ref();
        // `lane` counts lanes as the DSL does; a stereo lane is read on its
        // left side.
        let lane = lane * module.port_channels(port);
        Some(match module.port_view(port) {
            // An f64 port's lane starts with its nearest f32, which is
            // all a scope or meter needs.
            Some(PortView {
                data,
                lanes,
                words,
                processed,
            }) => Tap::View {
                data,
                stride: lanes * words,
                offset: (lane % lanes) * words,
                processed,
            },
            None => Tap::Module { module, port, lane },
        })
    }

    #[inline]
    pub(crate) fn get(&self, index: usize) -> f32 {
        match *self {
            // SAFETY: `data` points into the producer's block buffer, which
            // outlives the borrowed patch, and `index` is below the processed
            // slot (see `resolve`), so the offset stays inside the buffer.
            Tap::View {
                data,
                stride,
                offset,
                processed,
            } => unsafe {
                debug_assert!(index < (*processed).get());
                *data.add(index * stride + offset)
            },
            Tap::Module { module, port, lane } => module.get_value_at(port, lane, index),
        }
    }
}
