//! Analog-modelled filters: Sallen-Key, transistor ladder, diode ladder and
//! OTA cascade, each as a module per response with an output per slope, and
//! the leapfrog, a module per response.
//!
//! Each topology is a [`kernel::Kernel`]: its circuit's structure
//! discretised with trapezoidal integrators, its delay-free feedback loop
//! solved every sample, and its saturation where the circuit has it.
//! [`runner::run`] drives a kernel per lane, at twice the engine rate when
//! `highRes` is set.

mod cascade;
mod diode;
mod diode_modules;
mod kernel;
mod ladder_modules;
mod leapfrog;
mod leapfrog_modules;
mod module_macro;
mod ota_modules;
mod runner;
mod sallen_key;
mod sallen_key_modules;

use std::collections::HashMap;

use crate::params::ParamsDeserializer;
use crate::types::{Module, ModuleSchema, SampleableConstructor};

/// Calls `$each!` with every analog filter module type.
macro_rules! for_each_module {
    ($each:ident) => {
        $each!(
            sallen_key_modules::LpfSk,
            sallen_key_modules::HpfSk,
            sallen_key_modules::BpfSk,
            sallen_key_modules::NotchSk,
            ladder_modules::LpfLadder,
            ladder_modules::HpfLadder,
            ladder_modules::BpfLadder,
            ladder_modules::NotchLadder,
            ladder_modules::ApfLadder,
            diode_modules::LpfDiode,
            diode_modules::HpfDiode,
            diode_modules::BpfDiode,
            diode_modules::NotchDiode,
            ota_modules::LpfOta,
            ota_modules::HpfOta,
            ota_modules::BpfOta,
            ota_modules::NotchOta,
            ota_modules::ApfOta,
            leapfrog_modules::LpfLeapfrog,
            leapfrog_modules::NotchLeapfrog,
            leapfrog_modules::ApfLeapfrog
        )
    };
}

pub fn install_constructors(map: &mut HashMap<String, SampleableConstructor>) {
    macro_rules! install {
        ($($module:ty),*) => { $(<$module>::install_constructor(map);)* };
    }
    for_each_module!(install);
}

pub fn install_params_deserializers(map: &mut HashMap<String, ParamsDeserializer>) {
    macro_rules! install {
        ($($module:ty),*) => { $(<$module>::install_params_deserializer(map);)* };
    }
    for_each_module!(install);
}

pub fn schemas() -> Vec<ModuleSchema> {
    macro_rules! collect {
        ($($module:ty),*) => { vec![$(<$module>::get_schema()),*] };
    }
    for_each_module!(collect)
}
