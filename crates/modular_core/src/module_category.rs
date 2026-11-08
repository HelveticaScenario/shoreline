//! Reference categories for modules.

use napi_derive::napi;
use serde::{Deserialize, Serialize};
use strum::{EnumMessage, VariantArray};

/// A grouping for the module reference. Every module declares at least one
/// with `categories(...)` in its `#[module(...)]` attribute, and may belong
/// to several.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumMessage, VariantArray,
)]
#[serde(rename_all = "camelCase")]
#[napi(string_enum = "camelCase")]
// Variant docs must not contain commas: the napi codegen turns them into `|`.
// They are also the descriptions written to `moduleCategories.json`.
pub enum ModuleCategory {
    /// Sound sources driven by a frequency or V/Oct pitch.
    Oscillator,
    /// Phase ramps and warps plus the oscillators they drive.
    Phase,
    /// Filters and EQs that shape a signal's spectrum.
    Filter,
    /// Waveshapers and saturators.
    Distortion,
    /// Audio effects such as reverb and delay.
    Effect,
    /// Compressors and other level control.
    Dynamics,
    /// Envelope generators and followers.
    Envelope,
    /// Slowly moving control sources.
    Modulation,
    /// Random and chaotic sources.
    Random,
    /// Patterns and clocks for transport-synced sequencing.
    Sequencer,
    /// Gate and trigger processing.
    Gate,
    /// V/Oct pitch conversion plus scales and chords.
    Pitch,
    /// Drum voices and drum pattern generators.
    Drum,
    /// Playback of recorded audio and media files.
    Sampler,
    /// Combining and panning channels.
    Mixer,
    /// Arithmetic and range transforms on signals.
    Math,
    /// MIDI input.
    Midi,
    /// Signal routing and buffers.
    Utility,
}

/// A category and its description, as written to `moduleCategories.json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModuleCategoryInfo {
    pub name: ModuleCategory,
    pub description: &'static str,
}

impl ModuleCategory {
    /// The variant's `///` doc comment.
    pub fn description(self) -> &'static str {
        self.get_documentation()
            .expect("every ModuleCategory variant has a doc comment")
    }
}

/// Every category with its description, in reference order.
pub fn module_categories() -> Vec<ModuleCategoryInfo> {
    ModuleCategory::VARIANTS
        .iter()
        .map(|&name| ModuleCategoryInfo {
            name,
            description: name.description(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_category_has_a_one_line_description() {
        for category in ModuleCategory::VARIANTS {
            let doc = category.get_documentation();
            assert!(
                doc.is_some_and(|d| !d.is_empty() && !d.contains('\n')),
                "{category:?} needs a one-line `///` description, got {doc:?}"
            );
        }
    }

    #[test]
    fn categories_json_lists_names_and_descriptions() {
        let json = serde_json::to_value(module_categories()).unwrap();
        assert_eq!(json[0]["name"], "oscillator");
        assert_eq!(
            json[0]["description"],
            "Sound sources driven by a frequency or V/Oct pitch."
        );
        assert_eq!(
            json.as_array().unwrap().len(),
            ModuleCategory::VARIANTS.len()
        );
    }
}
