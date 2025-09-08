#![doc = r#"
limen-light

Minimal runtime for embedded and `no_std` targets. Cooperative `run_step` loop,
fixed-capacity buffers, no threads, and heap-free preprocessors where possible.
"#]

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use core::marker::PhantomData;

use limen_core::errors::RuntimeError;
use limen_core::runtime::BackpressurePolicy;
use limen_core::traits::{Model, OutputSink, Postprocessor, Preprocessor, SensorStream};
use limen_core::types::{SensorData, TensorInput};

pub struct RingBuffer<T, const CAPACITY: usize> {
    _phantom: PhantomData<T>,
}

impl<T, const CAPACITY: usize> RingBuffer<T, CAPACITY> {
    pub fn push(&mut self, _item: T, _policy: BackpressurePolicy) -> bool { todo!("RingBuffer::push") }
    pub fn pop(&mut self) -> Option<T> { todo!("RingBuffer::pop") }
    pub fn len(&self) -> usize { 0 }
}

pub struct RuntimeLight<S, P, M, Q, O>
where
    S: SensorStream, P: Preprocessor, M: Model, Q: Postprocessor, O: OutputSink,
{
    pub backpressure_policy: BackpressurePolicy,
    pub sensor: S,
    pub preprocessor: P,
    pub model: M,
    pub postprocessor: Q,
    pub sink: O,
}

impl<S, P, M, Q, O> RuntimeLight<S, P, M, Q, O>
where
    S: SensorStream, P: Preprocessor, M: Model, Q: Postprocessor, O: OutputSink,
{
    pub fn run_step(&mut self) -> Result<(), RuntimeError> { todo!("RuntimeLight::run_step") }
}

pub struct IdentityPreprocessorLight;

impl limen_core::traits::Preprocessor for IdentityPreprocessorLight {
    fn process(&mut self, _data: &SensorData) -> Result<Option<TensorInput>, limen_core::errors::ProcessingError> {
        todo!("IdentityPreprocessorLight::process")
    }
    fn reset(&mut self) -> Result<(), limen_core::errors::ProcessingError> { Ok(()) }
}

pub struct NoOpOutputSinkLight;

impl limen_core::traits::OutputSink for NoOpOutputSinkLight {
    fn publish(&mut self, _output: &limen_core::types::TensorOutput) -> Result<(), limen_core::errors::OutputError> { Ok(()) }
    fn flush(&mut self) -> Result<(), limen_core::errors::OutputError> { Ok(()) }
    fn close(&mut self) -> Result<(), limen_core::errors::OutputError> { Ok(()) }
}
