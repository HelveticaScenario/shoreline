//! The macros that stamp the analog filter modules.

/// Stamps one analog filter module from its kernel and response. `options`
/// builds its `Kernel::Options` from the params struct, and is left out for
/// a kernel without any. `drive` holds the drive param's description, and is
/// left out for a module without the param. `freq` names the cutoff param.
/// Each output is `field[tap] = "name": "description"`, one of them followed
/// by `, default`, where `tap` is its place in `Taps`.
macro_rules! analog_filter {
    (@drive $this:tt $drive_doc:literal) => {
        &$this.params.drive
    };
    (@drive $this:tt) => {
        &None
    };
    (
        docs = $docs:path,
        name = $name:literal,
        ident = $Struct:ident,
        kernel = $Kernel:ty,
        response = $response:expr,
        $(options = |$p:ident| $options:expr,)?
        $(drive = $drive_doc:literal,)?
        freq = $freq:ident,
        freq_doc = $freq_doc:literal,
        resonance_doc = $resonance_doc:literal,
        outputs = {
            $( $field:ident[$tap:expr] = $output:literal: $output_doc:literal $(, $default:ident)?; )+
        } $(,)?
    ) => {
        paste::paste! {
            #[params]
            #[derive(Clone, deserr::Deserr, schemars::JsonSchema, Connect, LaneCount)]
            #[serde(rename_all = "camelCase")]
            #[deserr(rename_all = camelCase, deny_unknown_fields)]
            struct [<$Struct Params>] {
                #[input("the sound to filter", type = audio)]
                input: crate::poly::PolySignal,
                #[input($freq_doc, type = frequency, range = (-3.71, ..))]
                $freq: crate::poly::PolySignal,
                #[input($resonance_doc, type = control, default = 0.0, range = (0.0, 5.0))]
                #[deserr(default)]
                resonance: Option<crate::poly::PolySignal>,
                $(
                    #[input($drive_doc, type = control, default = 0.0, range = (0.0, 5.0))]
                    #[deserr(default)]
                    drive: Option<crate::poly::PolySignal>,
                )?
                #[input(
                    "run at twice the engine rate: less aliasing from saturation and resonance, and truer tuning at high cutoffs; false costs less CPU"
                )]
                #[serde(default = "crate::dsp::utils::default_true")]
                #[deserr(default = crate::dsp::utils::default_true())]
                high_res: bool,
            }

            #[derive(Outputs, schemars::JsonSchema)]
            #[serde(rename_all = "camelCase")]
            struct [<$Struct Outputs>] {
                $(
                    #[output($output, $output_doc, type = cv $(, $default)?)]
                    $field: crate::poly::PolyOutput,
                )+
            }

            impl crate::dsp::filters::analog::runner::AnalogPorts for [<$Struct Outputs>] {
                fn taps(
                    connected: &<Self as crate::types::OutputStruct>::Connected,
                ) -> crate::dsp::filters::analog::kernel::Taps {
                    let mut taps = [false; 4];
                    $( taps[$tap] = connected.$field; )+
                    taps
                }

                fn ports(&mut self) -> [Option<&mut crate::poly::PolyOutput>; 4] {
                    let mut ports = [None, None, None, None];
                    $( ports[$tap] = Some(&mut self.$field); )+
                    ports
                }
            }

            #[module(
                name = $name,
                docs = $docs,
                args(input, $freq, resonance),
                has_init,
                patch_update,
                experimental
            )]
            pub struct $Struct {
                outputs: [<$Struct Outputs>],
                state: crate::dsp::filters::analog::runner::AnalogState<$Kernel>,
                lane_state: ::std::boxed::Box<
                    [crate::dsp::filters::analog::runner::AnalogLane<$Kernel>],
                >,
                params: [<$Struct Params>],
            }

            impl $Struct {
                fn init(&mut self, _sample_rate: f32) {
                    let base = crate::dsp::utils::rng::seed_base(self);
                    for (lane, state) in self.lane_state.iter_mut().enumerate() {
                        state.seed(base, lane);
                    }
                }

                /// The connected outputs, `highRes` and the kernel's options.
                fn key(&self) -> crate::dsp::filters::analog::runner::CoeffKey<$Kernel> {
                    use crate::dsp::filters::analog::runner::AnalogPorts;
                    (
                        [<$Struct Outputs>]::taps(&self.connected()),
                        self.params.high_res,
                        // Without options, the block is empty: `()`.
                        { $( let $p = &self.params; $options )? },
                    )
                }

                fn update(&mut self, sample_rate: f32) {
                    use crate::dsp::filters::analog::runner::AnalogPorts;
                    let lanes = self.lane_count();
                    let key = self.key();

                    crate::dsp::filters::analog::runner::run::<$Kernel>(
                        $response,
                        lanes,
                        &crate::dsp::filters::analog::runner::Inputs {
                            input: &self.params.input,
                            filter: crate::dsp::filters::resonant_svf::FilterParams {
                                cutoff: &self.params.$freq,
                                resonance: &self.params.resonance,
                                default_resonance: 0.0,
                            },
                            drive: $crate::dsp::filters::analog::module_macro::analog_filter!(
                                @drive self $($drive_doc)?
                            ),
                        },
                        key,
                        sample_rate,
                        &mut self.state,
                        &mut self.lane_state,
                        self.outputs.ports(),
                    );
                }
            }

            impl crate::types::PatchUpdateHandler for $Struct {
                fn on_patch_update(&mut self) {
                    let key = self.key();
                    self.state.on_patch_update(
                        key,
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
pub(crate) use analog_filter;

/// One analog filter of a kind: `lowpass` or `highpass` followed by `two`
/// (12 and 24 dB outputs, 12 the default) or `four` (6 to 24 dB, 24 the
/// default); `bandpass` or `notch` followed by the default output, `db12`
/// or `db24`, a notch optionally by what its resonance does; or `allpass`.
/// Then [`analog_filter`]'s fields up to `drive`.
macro_rules! analog_kind {
    (lowpass, $outputs:ident, $($head:tt)*) => {
        $crate::dsp::filters::analog::module_macro::analog_kind! {
            @slopes $outputs
            [
                "1-pole lowpass: 6 dB/oct",
                "2-pole lowpass: 12 dB/oct",
                "3-pole lowpass: 18 dB/oct",
                "4-pole lowpass: 24 dB/oct"
            ]
            [$($head)*]
        }
    };
    (highpass, $outputs:ident, $($head:tt)*) => {
        $crate::dsp::filters::analog::module_macro::analog_kind! {
            @slopes $outputs
            [
                "1-pole highpass: 6 dB/oct",
                "2-pole highpass: 12 dB/oct",
                "3-pole highpass: 18 dB/oct",
                "4-pole highpass: 24 dB/oct"
            ]
            [$($head)*]
        }
    };
    (bandpass, $default:ident, $($head:tt)*) => {
        $crate::dsp::filters::analog::module_macro::analog_kind! {
            @default $default {} {} center
            "narrows the band; self-oscillates from about 4.5"
            "2-pole bandpass: 6 dB/oct each side"
            "4-pole bandpass: 12 dB/oct each side"
            [$($head)*]
        }
    };
    (notch, $default:ident, $resonance_doc:literal, $($head:tt)*) => {
        $crate::dsp::filters::analog::module_macro::analog_kind! {
            @default $default {} {} center
            $resonance_doc
            "one null at the centre"
            "two nulls, half an octave either side of the centre"
            [$($head)*]
        }
    };
    (notch, $default:ident, $($head:tt)*) => {
        $crate::dsp::filters::analog::module_macro::analog_kind! {
            notch, $default,
            "sharpens the filter around the centre and thins the lows; self-oscillates from about 4.5",
            $($head)*
        }
    };
    (allpass, $($head:tt)*) => {
        $crate::dsp::filters::analog::module_macro::analog_kind! {
            @default db24 {} {} cutoff
            "sharpens the phase sweep; self-oscillates from about 4.5"
            "360° of phase shift across the cutoff, with a resonant peak there: one notch when mixed with the dry signal"
            "720° of phase shift across the cutoff at a constant level: two notches when mixed with the dry signal"
            [$($head)*]
        }
    };
    (@slopes two [$db6:literal, $db12:literal, $db18:literal, $db24:literal] $head:tt) => {
        $crate::dsp::filters::analog::module_macro::analog_kind! {
            @slope db12 {} {} $db12 $db24 $head
        }
    };
    (@slopes four [$db6:literal, $db12:literal, $db18:literal, $db24:literal] $head:tt) => {
        $crate::dsp::filters::analog::module_macro::analog_kind! {
            @slope db24 { db6[0] = "6db": $db6; } { db18[2] = "18db": $db18; } $db12 $db24 $head
        }
    };
    (@slope $default:ident $db6:tt $db18:tt $db12:literal $db24:literal $head:tt) => {
        $crate::dsp::filters::analog::module_macro::analog_kind! {
            @default $default $db6 $db18 cutoff
            "resonant peak at the cutoff; self-oscillates from about 4.5"
            $db12 $db24 $head
        }
    };
    (@default db12 $($rest:tt)*) => {
        $crate::dsp::filters::analog::module_macro::analog_kind! { @freq [default] [] $($rest)* }
    };
    (@default db24 $($rest:tt)*) => {
        $crate::dsp::filters::analog::module_macro::analog_kind! { @freq [] [default] $($rest)* }
    };
    (@freq $d12:tt $d24:tt $db6:tt $db18:tt cutoff $($rest:tt)*) => {
        $crate::dsp::filters::analog::module_macro::analog_kind! {
            @emit $d12 $d24 $db6 $db18 cutoff
            "cutoff frequency; tops out just under half the sample rate"
            $($rest)*
        }
    };
    (@freq $d12:tt $d24:tt $db6:tt $db18:tt center $($rest:tt)*) => {
        $crate::dsp::filters::analog::module_macro::analog_kind! {
            @emit $d12 $d24 $db6 $db18 center
            "centre frequency; tops out just under half the sample rate"
            $($rest)*
        }
    };
    (
        @emit [$($d12:ident)?] [$($d24:ident)?] {$($db6:tt)*} {$($db18:tt)*}
        $freq:ident $freq_doc:literal $resonance_doc:literal
        $db12_doc:literal $db24_doc:literal [$($head:tt)*]
    ) => {
        $crate::dsp::filters::analog::module_macro::analog_filter! {
            $($head)*
            drive = "level into the filter, up to +18 dB, pushing it into its saturation; the output comes down by half as much",
            freq = $freq,
            freq_doc = $freq_doc,
            resonance_doc = $resonance_doc,
            outputs = {
                $($db6)*
                db12[crate::dsp::filters::analog::kernel::DB12] = "12db": $db12_doc $(, $d12)?;
                $($db18)*
                db24[crate::dsp::filters::analog::kernel::DB24] = "24db": $db24_doc $(, $d24)?;
            },
        }
    };
}
pub(crate) use analog_kind;
