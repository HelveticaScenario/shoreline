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
        side: usize,
    ) -> Option<Self> {
        let module = sampleables.get(module_id)?.as_ref();
        // `lane` counts lanes as the DSL does; a mono lane reads the same on
        // either side.
        let channels = module.port_channels(port);
        Self::resolve_physical(
            sampleables,
            module_id,
            port,
            lane * channels + side.min(channels - 1),
        )
    }

    /// Like [`resolve`](Self::resolve), at physical lane `lane` of the port,
    /// where lane `l` side `c` is `l * channels + c`: a stereo out's meter
    /// reads its two sides as lanes 0 and 1.
    pub(crate) fn resolve_physical(
        sampleables: &'a SampleableMap,
        module_id: &str,
        port: &'a str,
        lane: usize,
    ) -> Option<Self> {
        let module = sampleables.get(module_id)?.as_ref();
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

#[cfg(test)]
mod tests {
    use super::Tap;
    use modular_core::Patch;
    use modular_core::types::ProcessingMode;
    use serde_json::json;

    fn cable(module: &str) -> serde_json::Value {
        json!({ "type": "cable", "module": module, "port": "output", "lane": 0 })
    }

    #[test]
    fn a_tap_reads_a_side_of_a_stereo_lane_and_either_side_of_a_mono_one() {
        let modules = [
            ("l", "$signal", json!({ "value": 1.0 })),
            ("r", "$signal", json!({ "value": 2.0 })),
            (
                "st",
                "$stereo",
                json!({ "left": cable("l"), "right": cable("r") }),
            ),
        ];
        let mut patch = Patch::new();
        patch
            .insert_modules(
                modules.iter().map(|(id, module_type, params)| {
                    let params =
                        crate::params_cache::deserialize_params(module_type, params.clone(), false)
                            .unwrap();
                    (
                        id.to_string(),
                        module_type.to_string(),
                        params,
                        ProcessingMode::Sample,
                    )
                }),
                48_000.0,
                1,
            )
            .unwrap();
        patch.connect_all();
        for module in patch.sampleables.values() {
            module.start_block();
        }
        for module in patch.sampleables.values() {
            module.ensure_processed_to(1);
        }
        let read = |id: &str, side| {
            Tap::resolve(&patch.sampleables, id, "output", 0, side)
                .unwrap()
                .get(0)
        };
        assert_eq!(read("st", 0), 1.0);
        assert_eq!(read("st", 1), 2.0);
        assert_eq!(read("l", 0), 1.0);
        assert_eq!(read("l", 1), 1.0);
    }
}
