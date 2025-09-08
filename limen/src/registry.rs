extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;

use limen_core::errors::RuntimeError;
use limen_core::traits::{
    ComputeBackendFactory, OutputSinkFactory, PlatformBackendFactory, PostprocessorFactory,
    PreprocessorFactory, SensorStreamFactory,
};

pub struct Registries {
    pub compute_backends: BTreeMap<String, Box<dyn ComputeBackendFactory>>,
    pub sensor_streams: BTreeMap<String, Box<dyn SensorStreamFactory>>,
    pub preprocessors: BTreeMap<String, Box<dyn PreprocessorFactory>>,
    pub postprocessors: BTreeMap<String, Box<dyn PostprocessorFactory>>,
    pub output_sinks: BTreeMap<String, Box<dyn OutputSinkFactory>>,
    pub platforms: BTreeMap<String, Box<dyn PlatformBackendFactory>>,
}

impl Registries {
    pub fn new() -> Self {
        Self {
            compute_backends: BTreeMap::new(),
            sensor_streams: BTreeMap::new(),
            preprocessors: BTreeMap::new(),
            postprocessors: BTreeMap::new(),
            output_sinks: BTreeMap::new(),
            platforms: BTreeMap::new(),
        }
    }

    pub fn register_compute_backend(&mut self, key: &str, factory: Box<dyn ComputeBackendFactory>) -> Result<(), RuntimeError> {
        if self.compute_backends.contains_key(key) {
            return Err(RuntimeError::InitializationFailed(format!("duplicate compute backend key: {key}")));
        }
        self.compute_backends.insert(key.to_string(), factory);
        Ok(())
    }

    pub fn register_sensor_stream(&mut self, key: &str, factory: Box<dyn SensorStreamFactory>) -> Result<(), RuntimeError> {
        if self.sensor_streams.contains_key(key) {
            return Err(RuntimeError::InitializationFailed(format!("duplicate sensor stream key: {key}")));
        }
        self.sensor_streams.insert(key.to_string(), factory);
        Ok(())
    }

    pub fn register_preprocessor(&mut self, key: &str, factory: Box<dyn PreprocessorFactory>) -> Result<(), RuntimeError> {
        if self.preprocessors.contains_key(key) {
            return Err(RuntimeError::InitializationFailed(format!("duplicate preprocessor key: {key}")));
        }
        self.preprocessors.insert(key.to_string(), factory);
        Ok(())
    }

    pub fn register_postprocessor(&mut self, key: &str, factory: Box<dyn PostprocessorFactory>) -> Result<(), RuntimeError> {
        if self.postprocessors.contains_key(key) {
            return Err(RuntimeError::InitializationFailed(format!("duplicate postprocessor key: {key}")));
        }
        self.postprocessors.insert(key.to_string(), factory);
        Ok(())
    }

    pub fn register_output_sink(&mut self, key: &str, factory: Box<dyn OutputSinkFactory>) -> Result<(), RuntimeError> {
        if self.output_sinks.contains_key(key) {
            return Err(RuntimeError::InitializationFailed(format!("duplicate output sink key: {key}")));
        }
        self.output_sinks.insert(key.to_string(), factory);
        Ok(())
    }

    pub fn register_platform(&mut self, key: &str, factory: Box<dyn PlatformBackendFactory>) -> Result<(), RuntimeError> {
        if self.platforms.contains_key(key) {
            return Err(RuntimeError::InitializationFailed(format!("duplicate platform key: {key}")));
        }
        self.platforms.insert(key.to_string(), factory);
        Ok(())
    }
}
