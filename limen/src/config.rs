extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use limen_core::traits::configuration::{
    OutputSinkConfiguration, PostprocessorConfiguration, PreprocessorConfiguration, SensorStreamConfiguration,
};

#[derive(Clone, Debug)]
pub struct RuntimeConfiguration {
    pub backpressure_policy: String,
    pub queue_capacity: usize,
    pub sensor: SensorStreamConfiguration,
    pub preprocessors: Vec<PreprocessorConfiguration>,
    pub model: ModelConfiguration,
    pub postprocessors: Vec<PostprocessorConfiguration>,
    pub sinks: Vec<OutputSinkConfiguration>,
    pub secure_mode: bool,
}

#[derive(Clone, Debug)]
pub struct ModelConfiguration {
    pub backend: String,
    pub model_bytes_path: Option<String>,
    pub model_bundle_path: Option<String>,
    pub model_signature_path: Option<String>,
    pub options: BTreeMap<String, String>,
}
