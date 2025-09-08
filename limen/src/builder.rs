extern crate alloc;

use alloc::boxed::Box;

use limen_core::errors::RuntimeError;
use limen_core::runtime::{BackpressurePolicy, Runtime};
use limen_core::traits::{Model, OutputSink, Postprocessor, Preprocessor, SensorStream};

use crate::config::RuntimeConfiguration;
use crate::registry::Registries;

pub struct RuntimeBuilder;

impl RuntimeBuilder {
    pub fn build_from_configuration(
        _registries: &Registries,
        _configuration: &RuntimeConfiguration,
    ) -> Result<
        Runtime<
            Box<dyn SensorStream>,
            Box<dyn Preprocessor>,
            Box<dyn Model>,
            Box<dyn Postprocessor>,
            Box<dyn OutputSink>
        >,
        RuntimeError,
    > {
        todo!("RuntimeBuilder::build_from_configuration is not implemented in the specification scaffold")
    }

    pub fn parse_backpressure_policy(_text: &str) -> Result<BackpressurePolicy, RuntimeError> {
        todo!("RuntimeBuilder::parse_backpressure_policy is not implemented in the specification scaffold")
    }
}
