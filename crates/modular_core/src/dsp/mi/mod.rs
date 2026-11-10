//! Ports of Mutable Instruments modules (`$mi.*`), from `pichenettes/eurorack`
//! under the MIT license (see `LICENSE`).

use std::collections::HashMap;

use crate::params::ParamsDeserializer;
use crate::types::{Module, ModuleSchema, SampleableConstructor};

pub mod core;
pub mod formant;
pub mod two_op;
pub mod waveshaper;
pub mod z_osc;

pub fn install_constructors(map: &mut HashMap<String, SampleableConstructor>) {
    formant::Formant::install_constructor(map);
    two_op::TwoOp::install_constructor(map);
    waveshaper::Waveshaper::install_constructor(map);
    z_osc::ZOsc::install_constructor(map);
}

pub fn install_params_deserializers(map: &mut HashMap<String, ParamsDeserializer>) {
    formant::Formant::install_params_deserializer(map);
    two_op::TwoOp::install_params_deserializer(map);
    waveshaper::Waveshaper::install_params_deserializer(map);
    z_osc::ZOsc::install_params_deserializer(map);
}

pub fn schemas() -> Vec<ModuleSchema> {
    vec![
        formant::Formant::get_schema(),
        two_op::TwoOp::get_schema(),
        waveshaper::Waveshaper::get_schema(),
        z_osc::ZOsc::get_schema(),
    ]
}
