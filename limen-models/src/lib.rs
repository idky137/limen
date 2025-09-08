#![doc = r#"
limen-models

Backend implementations that satisfy `ComputeBackend` and `Model`.
This crate only adapts engines; it does not implement kernels.
"#]

use limen_core::errors::InferenceError;
use limen_core::traits::{ComputeBackend, ComputeBackendConfiguration, Model};
use limen_core::types::{DataType, ModelMetadata, TensorInput, TensorOutput};

#[cfg(feature = "tract")]
pub struct TractComputeBackend;

#[cfg(feature = "tract")]
impl ComputeBackend for TractComputeBackend {
    fn name(&self) -> &'static str { "tract" }
    fn configure(&mut self, _configuration: ComputeBackendConfiguration) -> Result<(), InferenceError> { Ok(()) }
    fn supported_data_types(&self) -> &'static [DataType] {
        static SUPPORTED: &[DataType] = &[DataType::F32, DataType::I8, DataType::U8];
        SUPPORTED
    }
    fn load_model(&mut self, _model_bytes: &[u8]) -> Result<Box<dyn Model>, InferenceError> {
        Ok(Box::new(TractModel { metadata: ModelMetadata { name: "tract_model".into(), inputs: vec![], outputs: vec![] } }))
    }
    fn warm_up(&mut self) -> Result<(), InferenceError> { Ok(()) }
}

#[cfg(feature = "tract")]
pub struct TractModel { pub metadata: ModelMetadata }

#[cfg(feature = "tract")]
impl Model for TractModel {
    fn metadata(&self) -> &ModelMetadata { &self.metadata }
    fn infer(&mut self, _input: &TensorInput) -> Result<TensorOutput, InferenceError> {
        todo!("TractModel::infer is not implemented in the specification scaffold")
    }
    fn unload(self: Box<Self>) -> Result<(), InferenceError> { Ok(()) }
}

#[cfg(feature = "onnxruntime")]
pub struct OnnxRuntimeComputeBackend;

#[cfg(feature = "onnxruntime")]
impl ComputeBackend for OnnxRuntimeComputeBackend {
    fn name(&self) -> &'static str { "onnxruntime" }
    fn configure(&mut self, _configuration: ComputeBackendConfiguration) -> Result<(), InferenceError> { Ok(()) }
    fn supported_data_types(&self) -> &'static [DataType] {
        static SUPPORTED: &[DataType] = &[DataType::F32, DataType::I8, DataType::U8];
        SUPPORTED
    }
    fn load_model(&mut self, _model_bytes: &[u8]) -> Result<Box<dyn Model>, InferenceError> {
        Ok(Box::new(OnnxRuntimeModel { metadata: ModelMetadata { name: "onnx_model".into(), inputs: vec![], outputs: vec![] } }))
    }
    fn warm_up(&mut self) -> Result<(), InferenceError> { Ok(()) }
}

#[cfg(feature = "onnxruntime")]
pub struct OnnxRuntimeModel { pub metadata: ModelMetadata }

#[cfg(feature = "onnxruntime")]
impl Model for OnnxRuntimeModel {
    fn metadata(&self) -> &ModelMetadata { &self.metadata }
    fn infer(&mut self, _input: &TensorInput) -> Result<TensorOutput, InferenceError> {
        todo!("OnnxRuntimeModel::infer is not implemented in the specification scaffold")
    }
    fn unload(self: Box<Self>) -> Result<(), InferenceError> { Ok(()) }
}

pub mod register {
    pub fn register_all() {
        // Registration wiring placeholder.
    }
}
