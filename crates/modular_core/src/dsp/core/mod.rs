use std::collections::HashMap;

use crate::params::ParamsDeserializer;
use crate::types::{Module, ModuleSchema, SampleableConstructor};

pub mod audio_in;
pub mod clock;
pub mod mix;
pub mod set_lanes;
pub mod signal;
pub mod stereo;
pub mod stereo_mixer;

pub fn install_constructors(map: &mut HashMap<String, SampleableConstructor>) {
    signal::Signal::install_constructor(map);
    mix::Mix::install_constructor(map);
    set_lanes::SetLanes::install_constructor(map);
    stereo::Stereo::install_constructor(map);
    stereo::Mono::install_constructor(map);
    stereo::Left::install_constructor(map);
    stereo::Right::install_constructor(map);
    stereo_mixer::StereoMixer::install_constructor(map);
    clock::Clock::install_constructor(map);
}

pub fn install_params_deserializers(map: &mut HashMap<String, ParamsDeserializer>) {
    signal::Signal::install_params_deserializer(map);
    mix::Mix::install_params_deserializer(map);
    set_lanes::SetLanes::install_params_deserializer(map);
    stereo::Stereo::install_params_deserializer(map);
    stereo::Mono::install_params_deserializer(map);
    stereo::Left::install_params_deserializer(map);
    stereo::Right::install_params_deserializer(map);
    stereo_mixer::StereoMixer::install_params_deserializer(map);
    clock::Clock::install_params_deserializer(map);
}

pub fn schemas() -> Vec<ModuleSchema> {
    vec![
        signal::Signal::get_schema(),
        mix::Mix::get_schema(),
        set_lanes::SetLanes::get_schema(),
        stereo::Stereo::get_schema(),
        stereo::Mono::get_schema(),
        stereo::Left::get_schema(),
        stereo::Right::get_schema(),
        stereo_mixer::StereoMixer::get_schema(),
        clock::Clock::get_schema(),
    ]
}
