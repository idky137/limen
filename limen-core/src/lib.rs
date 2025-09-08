#![doc = r#" 
limen-core

Foundational crate for the Limen runtime. Provides canonical tensor and sensor
types, error types, trait contracts, and the core runtime for a linear pipeline:
SensorStream → Preprocessor chain → Model → Postprocessor chain → OutputSink chain.

Features
- `std` (default): enables standard library usage.
- `alloc`: enables heap-backed containers for `no_std` targets.
- `serde`: enables (de)serialization on public types where safe.
- `tracing`: enables structured tracing spans around every stage call.
- `metrics`: enables counters and histograms for throughput and latency.
"#]

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Common data types and tensor representations.
pub mod types {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum DataType { F32, F16, I8, U8, I16, I32 }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Shape { pub rank: usize, pub dimensions: Vec<usize> }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Stride { pub elements_per_axis: Vec<isize> }

    #[derive(Clone, Debug, PartialEq)]
    pub struct QuantizationParameters { pub scale: f32, pub zero_point: i32 }

    #[derive(Clone, Debug, PartialEq)]
    pub struct TensorInput {
        pub element_type: DataType,
        pub shape: Shape,
        pub strides: Option<Stride>,
        pub quantization: Option<QuantizationParameters>,
        pub bytes: Cow<'static, [u8]>,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct TensorOutput {
        pub element_type: DataType,
        pub shape: Shape,
        pub strides: Option<Stride>,
        pub quantization: Option<QuantizationParameters>,
        pub bytes: Vec<u8>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Timestamp(pub u128);

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct SequenceNumber(pub u64);

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct SensorMetadata {
        pub name: String,
        pub schema_hint: Option<String>,
        pub nominal_hertz: Option<f32>,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct SensorData {
        pub timestamp: Timestamp,
        pub sequence_number: SequenceNumber,
        pub metadata: SensorMetadata,
        pub payload: Vec<u8>,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct ModelMetadata { pub name: String, pub inputs: Vec<ModelPort>, pub outputs: Vec<ModelPort> }

    #[derive(Clone, Debug, PartialEq)]
    pub struct ModelPort {
        pub element_type: DataType,
        pub shape: Shape,
        pub quantization: Option<QuantizationParameters>,
        pub label: Option<String>,
    }
}

pub mod errors {
    use super::types::{DataType, Shape};

    #[derive(Debug)]
    pub enum InferenceError {
        BackendUnavailable(String),
        UnsupportedDataType { requested: DataType },
        ShapeMismatch { expected: Shape, found: Shape },
        InvalidModel(String),
        ExecutionFailed(String),
        WarmUpFailed(String),
        ResourceExhausted(String),
        Other(String),
    }

    #[derive(Debug)]
    pub enum SensorError {
        OpenFailed(String),
        ReadFailed(String),
        EndOfStream,
        ResetFailed(String),
        CloseFailed(String),
        Other(String),
    }

    #[derive(Debug)]
    pub enum ProcessingError {
        ConfigurationInvalid(String),
        TransformFailed(String),
        NotReady,
        Other(String),
    }

    #[derive(Debug)]
    pub enum OutputError {
        PublishFailed(String),
        FlushFailed(String),
        CloseFailed(String),
        Other(String),
    }

    #[derive(Debug)]
    pub enum RuntimeError {
        InitializationFailed(String),
        BackpressureViolation(String),
        StageFailed { stage: &'static str, cause: String },
        StopRequested,
        Other(String),
    }
}

pub mod traits {
    use super::errors::{InferenceError, OutputError, ProcessingError, RuntimeError, SensorError};
    use super::types::{DataType, ModelMetadata, TensorInput, TensorOutput, SensorData};
    use alloc::collections::BTreeMap;

    pub mod configuration {
        use alloc::collections::BTreeMap;
        use alloc::string::String;

        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct SensorStreamConfiguration { pub name: String, pub parameters: BTreeMap<String, String> }

        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct PreprocessorConfiguration { pub name: String, pub parameters: BTreeMap<String, String> }

        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct PostprocessorConfiguration { pub name: String, pub parameters: BTreeMap<String, String> }

        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct OutputSinkConfiguration { pub name: String, pub parameters: BTreeMap<String, String> }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ComputeBackendConfiguration {
        pub intra_operation_threads: Option<usize>,
        pub inter_operation_threads: Option<usize>,
        pub memory_arena_bytes: Option<usize>,
        pub additional_options: BTreeMap<String, String>,
    }

    pub trait ComputeBackend: Send + Sync {
        fn name(&self) -> &'static str;
        fn configure(&mut self, configuration: ComputeBackendConfiguration) -> Result<(), InferenceError>;
        fn supported_data_types(&self) -> &'static [DataType];
        fn load_model(&mut self, model_bytes: &[u8]) -> Result<Box<dyn Model>, InferenceError>;
        fn warm_up(&mut self) -> Result<(), InferenceError>;
    }

    pub trait ComputeBackendFactory: Send + Sync {
        fn create(&self) -> Result<Box<dyn ComputeBackend>, InferenceError>;
    }

    pub trait Model: Send {
        fn metadata(&self) -> &ModelMetadata;
        fn infer(&mut self, input: &TensorInput) -> Result<TensorOutput, InferenceError>;
        fn unload(self: Box<Self>) -> Result<(), InferenceError>;
    }

    pub trait SensorStream: Send {
        fn open(&mut self) -> Result<(), SensorError>;
        fn read_next(&mut self) -> Result<Option<SensorData>, SensorError>;
        fn close(&mut self) -> Result<(), SensorError>;
        fn reset(&mut self) -> Result<(), SensorError>;
    }

    pub trait SensorStreamFactory: Send + Sync {
        fn create(&self, configuration: configuration::SensorStreamConfiguration) -> Result<Box<dyn SensorStream>, SensorError>;
    }

    pub trait Preprocessor: Send {
        fn process(&mut self, data: &SensorData) -> Result<Option<TensorInput>, ProcessingError>;
        fn reset(&mut self) -> Result<(), ProcessingError>;
    }

    pub trait PreprocessorFactory: Send + Sync {
        fn create(&self, configuration: configuration::PreprocessorConfiguration) -> Result<Box<dyn Preprocessor>, ProcessingError>;
    }

    pub trait Postprocessor: Send {
        fn process(&mut self, model_output: &TensorOutput) -> Result<Option<TensorOutput>, ProcessingError>;
        fn reset(&mut self) -> Result<(), ProcessingError>;
    }

    pub trait PostprocessorFactory: Send + Sync {
        fn create(&self, configuration: configuration::PostprocessorConfiguration) -> Result<Box<dyn Postprocessor>, ProcessingError>;
    }

    pub trait OutputSink: Send {
        fn publish(&mut self, output: &TensorOutput) -> Result<(), OutputError>;
        fn flush(&mut self) -> Result<(), OutputError>;
        fn close(&mut self) -> Result<(), OutputError>;
    }

    pub trait OutputSinkFactory: Send + Sync {
        fn create(&self, configuration: configuration::OutputSinkConfiguration) -> Result<Box<dyn OutputSink>, OutputError>;
    }

    pub trait PlatformBackend: Send + Sync {
        fn description(&self) -> &'static str;
        fn constraints(&self) -> PlatformConstraints;
        fn initialize(&mut self) -> Result<(), RuntimeError>;
        fn shutdown(&mut self) -> Result<(), RuntimeError>;
    }

    pub trait PlatformBackendFactory: Send + Sync {
        fn create(&self) -> Result<Box<dyn PlatformBackend>, RuntimeError>;
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct PlatformConstraints {
        pub maximum_memory_bytes: Option<usize>,
        pub general_purpose_input_output_available: bool,
        pub preferred_thread_count: Option<usize>,
    }

    pub use configuration::{PreprocessorConfiguration, SensorStreamConfiguration, PostprocessorConfiguration, OutputSinkConfiguration};
}

pub mod runtime {
    use super::errors::RuntimeError;
    use super::traits::{Model, OutputSink, Postprocessor, Preprocessor, SensorStream};
    use super::types::Timestamp;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum BackpressurePolicy { Block, DropNewest, DropOldest }

    #[derive(Clone, Debug, Default)]
    pub struct FlowStatistics {
        pub items_enqueued: u64,
        pub items_dequeued: u64,
        pub items_dropped: u64,
        pub last_enqueue_timestamp: Option<Timestamp>,
        pub last_dequeue_timestamp: Option<Timestamp>,
    }

    #[derive(Clone, Debug)]
    pub struct RuntimeHealth {
        pub running: bool,
        pub sensor_queue_depth: usize,
        pub preprocessor_queue_depth: usize,
        pub model_queue_depth: usize,
        pub postprocessor_queue_depth: usize,
        pub sink_queue_depth: usize,
        pub flow_statistics: FlowStatistics,
    }

    #[derive(Clone, Debug)]
    pub struct LinearGraphDescription {
        pub sensor_name: alloc::string::String,
        pub preprocessor_names: alloc::vec::Vec<alloc::string::String>,
        pub model_name: alloc::string::String,
        pub postprocessor_names: alloc::vec::Vec<alloc::string::String>,
        pub sink_names: alloc::vec::Vec<alloc::string::String>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum RuntimeState { Initialized, Running, Stopping, Stopped }

    pub struct Runtime<S, P, M, Q, O>
    where
        S: SensorStream, P: Preprocessor, M: Model, Q: Postprocessor, O: OutputSink,
    {
        pub backpressure_policy: BackpressurePolicy,
        pub queue_capacity: usize,
        pub statistics: FlowStatistics,
        pub state: RuntimeState,
        pub sensor: S,
        pub preprocessor: P,
        pub model: M,
        pub postprocessor: Q,
        pub sink: O,
    }

    impl<S, P, M, Q, O> Runtime<S, P, M, Q, O>
    where
        S: SensorStream, P: Preprocessor, M: Model, Q: Postprocessor, O: OutputSink,
    {
        pub fn new(sensor: S, preprocessor: P, model: M, postprocessor: Q, sink: O, backpressure_policy: BackpressurePolicy, queue_capacity: usize) -> Self {
            Self {
                backpressure_policy,
                queue_capacity,
                statistics: FlowStatistics::default(),
                state: RuntimeState::Initialized,
                sensor, preprocessor, model, postprocessor, sink,
            }
        }

        pub fn run(&mut self) -> Result<(), RuntimeError> {
            todo!("Runtime::run is not implemented in the specification scaffold")
        }

        pub fn stop(&mut self) -> Result<(), RuntimeError> {
            todo!("Runtime::stop is not implemented in the specification scaffold")
        }

        pub fn health(&self) -> RuntimeHealth {
            RuntimeHealth {
                running: matches!(self.state, RuntimeState::Running),
                sensor_queue_depth: 0,
                preprocessor_queue_depth: 0,
                model_queue_depth: 0,
                postprocessor_queue_depth: 0,
                sink_queue_depth: 0,
                flow_statistics: self.statistics.clone(),
            }
        }
    }
}
