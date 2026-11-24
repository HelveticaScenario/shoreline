//! Stamps `$lpf`, `$hpf`, `$bpf` and `$notch` from the shared SVF engine.

/// One clean filter module. `freq` names the cutoff param, with its
/// description and an optional `default`; `resonance` gives the default
/// resonance and its description; `db12` and `db24` describe the outputs.
macro_rules! svf_filter {
    (
        docs = $docs:path,
        name = $name:literal,
        ident = $Struct:ident,
        response = $response:ident,
        freq = $freq:ident: $freq_doc:literal $(, default = $freq_default:literal)?,
        resonance = $default_resonance:literal: $resonance_doc:literal,
        db12 = $db12_doc:literal,
        db24 = $db24_doc:literal $(,)?
    ) => {
        paste::paste! {
            #[params]
            #[derive(Clone, deserr::Deserr, schemars::JsonSchema, Connect, LaneCount)]
            #[serde(rename_all = "camelCase")]
            #[deserr(rename_all = camelCase, deny_unknown_fields)]
            struct [<$Struct Params>] {
                #[input("the sound to filter", type = audio)]
                input: crate::poly::PolySignal,
                #[input(
                    $freq_doc,
                    type = frequency,
                    $(default = $freq_default,)?
                    range = (-3.71, ..),
                )]
                $freq: crate::poly::PolySignal,
                #[input(
                    $resonance_doc,
                    type = control,
                    default = $default_resonance,
                    range = (0.0, ..),
                    control_range = (0.0, 5.0),
                )]
                #[deserr(default)]
                resonance: Option<crate::poly::PolySignal>,
            }

            #[derive(Outputs, schemars::JsonSchema)]
            #[serde(rename_all = "camelCase")]
            struct [<$Struct Outputs>] {
                #[output("12db", $db12_doc, type = cv, default)]
                db12: crate::poly::PolyOutput,
                #[output("24db", $db24_doc, type = cv)]
                db24: crate::poly::PolyOutput,
            }

            #[module(
                name = $name,
                docs = $docs,
                patch_update,
                args(input, $freq, resonance)
            )]
            pub struct $Struct {
                outputs: [<$Struct Outputs>],
                state: crate::dsp::filters::resonant_svf::SvfFilterState,
                lane_state: ::std::boxed::Box<[crate::dsp::filters::resonant_svf::SvfLane]>,
                params: [<$Struct Params>],
            }

            impl $Struct {
                fn taps(&self) -> crate::dsp::filters::resonant_svf::Taps {
                    let connected = self.connected();
                    [connected.db12, connected.db24]
                }

                fn update(&mut self, sample_rate: f32) {
                    let lanes = self.lane_count();
                    let taps = self.taps();
                    crate::dsp::filters::resonant_svf::run(
                        crate::dsp::filters::resonant_svf::Response::$response,
                        lanes,
                        &self.params.input,
                        &crate::dsp::filters::resonant_svf::FilterParams {
                            cutoff: &self.params.$freq,
                            resonance: &self.params.resonance,
                            default_resonance: $default_resonance,
                        },
                        taps,
                        sample_rate,
                        &mut self.state,
                        &mut self.lane_state,
                        [&mut self.outputs.db12, &mut self.outputs.db24],
                    );
                }
            }

            impl crate::types::PatchUpdateHandler for $Struct {
                fn on_patch_update(&mut self) {
                    let taps = self.taps();
                    self.state.on_patch_update(
                        taps,
                        &self.params.$freq,
                        &self.params.resonance,
                        &mut self.lane_state,
                    );
                }
            }

            message_handlers!(impl $Struct {});
        }
    };
}
pub(crate) use svf_filter;
