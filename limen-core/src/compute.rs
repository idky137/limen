//! Compute backend and model traits (dyn-free, explicit; no defaults).
//!
//! Backends implement `ComputeBackend<InP, OutP>` and return a concrete `Model`
//! that implements `ComputeModel<InP, OutP>`. The hot path is `infer_one`,
//! which performs exactly one synchronous inference step for a single input.
//!
//! Backends are monomorphized by generics (no dynamic dispatch). The model API
//! is intentionally decoupled from graph/queue details so nodes own batching,
//! backpressure, and telemetry.

use crate::errors::InferenceError;
use crate::memory::MemoryClass;
use crate::message::payload::Payload;

/// Capability descriptor of a compute backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendCapabilities {
    /// Whether the backend supports device streams (async/event completion).
    pub device_streams: bool,
    /// Maximum supported batch size, if any.
    pub max_batch: Option<usize>,
    /// Bitfield for supported data types (backend-defined; optional use).
    pub dtype_mask: u64,
}

/// Model metadata describing input/output shapes and preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelMetadata {
    /// Preferred input memory class (Host/Pinned/Device).
    pub preferred_input: MemoryClass,
    /// Preferred output memory class.
    pub preferred_output: MemoryClass,
    /// Optional maximum input size in bytes (admission hint).
    pub max_input_bytes: Option<usize>,
    /// Optional maximum output size in bytes.
    pub max_output_bytes: Option<usize>,
}

/// A loaded model that can perform inference.
pub trait ComputeModel<InP: Payload, OutP: Payload> {
    /// Prepare internal state (allocate work buffers, compile kernels, etc.).
    fn init(&mut self) -> Result<(), InferenceError>;

    /// Single-item inference (1×1).
    fn infer_one(&mut self, inp: &InP, out: &mut OutP) -> Result<(), InferenceError>;

    /// Optional: batched inference. Default loops `infer_one`.
    #[inline]
    fn infer_batch(&mut self, inps: &[InP], outs: &mut [OutP]) -> Result<(), InferenceError> {
        for (i, o) in inps.iter().zip(outs.iter_mut()) {
            self.infer_one(i, o)?;
        }
        Ok(())
    }

    /// Ensure outstanding device work is complete (if any).
    fn drain(&mut self) -> Result<(), InferenceError>;

    /// Reset internal state to a known baseline (drop caches, etc.).
    fn reset(&mut self) -> Result<(), InferenceError>;

    /// Return model metadata (I/O placement preferences, limits).
    fn metadata(&self) -> ModelMetadata;
}

/// A dyn-free engine that constructs models and reports capabilities.
pub trait ComputeBackend<InP: Payload, OutP: Payload> {
    /// Concrete model type (no trait objects).
    type Model: ComputeModel<InP, OutP>;

    /// Backend-specific error.
    type Error;

    /// Backend-chosen borrowed descriptor used to load a model.
    ///
    /// Examples:
    /// - on `std`:    `type ModelDescriptor<'desc> = &'desc ModelArtifact;`
    /// - on `no_std`: `type ModelDescriptor<'desc> = &'desc [u8];`
    type ModelDescriptor<'desc>
    where
        Self: 'desc;

    /// Capability report.
    fn capabilities(&self) -> BackendCapabilities;

    /// Load a model from a descriptor.
    fn load_model<'desc>(
        &self,
        desc: Self::ModelDescriptor<'desc>,
    ) -> Result<Self::Model, Self::Error>;
}

/// A simple artifact passed to backends for model creation (POC-friendly).
#[cfg(feature = "std")]
#[derive(Debug, Clone)]
pub struct ModelArtifact {
    /// Raw bytes of a model file or an engine-specific blob.
    pub bytes: std::sync::Arc<Vec<u8>>,
    /// Optional label or path hint.
    pub label: Option<String>,
}

#[cfg(feature = "std")]
impl ModelArtifact {
    /// Construct from raw bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            bytes: std::sync::Arc::new(bytes),
            label: None,
        }
    }

    /// Convenience: load from a file path.
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        Ok(Self::from_bytes(bytes))
    }
}
