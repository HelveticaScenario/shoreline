//! Ports of Mutable Instruments modules (`$mi.*`), from `pichenettes/eurorack`
//! under the MIT license (see `LICENSE`).

use std::collections::HashMap;

use crate::params::ParamsDeserializer;
use crate::types::{Module, ModuleSchema, SampleableConstructor};

pub mod additive;
pub mod chorus;
pub mod core;
pub mod divide_down;
pub mod formant;
pub mod modal;
pub mod particles;
pub mod phase_distortion;
pub mod string;
pub mod swarm;
pub mod two_op;
pub mod wave_terrain;
pub mod waveshaper;
pub mod z_osc;

pub fn install_constructors(map: &mut HashMap<String, SampleableConstructor>) {
    additive::Additive::install_constructor(map);
    chorus::Chorus::install_constructor(map);
    divide_down::DivideDown::install_constructor(map);
    formant::Formant::install_constructor(map);
    modal::Modal::install_constructor(map);
    particles::MiParticles::install_constructor(map);
    phase_distortion::PhaseDistortion::install_constructor(map);
    string::MiString::install_constructor(map);
    swarm::Swarm::install_constructor(map);
    two_op::TwoOp::install_constructor(map);
    wave_terrain::WaveTerrain::install_constructor(map);
    waveshaper::Waveshaper::install_constructor(map);
    z_osc::ZOsc::install_constructor(map);
}

pub fn install_params_deserializers(map: &mut HashMap<String, ParamsDeserializer>) {
    additive::Additive::install_params_deserializer(map);
    chorus::Chorus::install_params_deserializer(map);
    divide_down::DivideDown::install_params_deserializer(map);
    formant::Formant::install_params_deserializer(map);
    modal::Modal::install_params_deserializer(map);
    particles::MiParticles::install_params_deserializer(map);
    phase_distortion::PhaseDistortion::install_params_deserializer(map);
    string::MiString::install_params_deserializer(map);
    swarm::Swarm::install_params_deserializer(map);
    two_op::TwoOp::install_params_deserializer(map);
    wave_terrain::WaveTerrain::install_params_deserializer(map);
    waveshaper::Waveshaper::install_params_deserializer(map);
    z_osc::ZOsc::install_params_deserializer(map);
}

pub fn schemas() -> Vec<ModuleSchema> {
    vec![
        additive::Additive::get_schema(),
        chorus::Chorus::get_schema(),
        divide_down::DivideDown::get_schema(),
        formant::Formant::get_schema(),
        modal::Modal::get_schema(),
        particles::MiParticles::get_schema(),
        phase_distortion::PhaseDistortion::get_schema(),
        string::MiString::get_schema(),
        swarm::Swarm::get_schema(),
        two_op::TwoOp::get_schema(),
        wave_terrain::WaveTerrain::get_schema(),
        waveshaper::Waveshaper::get_schema(),
        z_osc::ZOsc::get_schema(),
    ]
}
