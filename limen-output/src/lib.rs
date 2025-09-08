#![doc = r#"
limen-output

Output sinks that publish or persist `TensorOutput`. Sinks must not block
indefinitely in `publish` and should honor backpressure semantics at the host.
"#]

extern crate alloc;

use alloc::string::String;

use limen_core::errors::OutputError;
use limen_core::traits::OutputSink;
use limen_core::types::TensorOutput;

pub struct StandardOutputSink { pub pretty_print: bool }

impl OutputSink for StandardOutputSink {
    fn publish(&mut self, _output: &TensorOutput) -> Result<(), OutputError> {
        todo!("StandardOutputSink::publish is not implemented in the specification scaffold")
    }
    fn flush(&mut self) -> Result<(), OutputError> { Ok(()) }
    fn close(&mut self) -> Result<(), OutputError> { Ok(()) }
}

pub struct FileSink { pub path: String, pub append: bool, pub buffer_bytes: Option<usize> }

impl OutputSink for FileSink {
    fn publish(&mut self, _output: &TensorOutput) -> Result<(), OutputError> {
        todo!("FileSink::publish is not implemented in the specification scaffold")
    }
    fn flush(&mut self) -> Result<(), OutputError> { Ok(()) }
    fn close(&mut self) -> Result<(), OutputError> { Ok(()) }
}

#[cfg(feature = "message-queuing-telemetry-transport")]
pub struct MessageQueuingTelemetryTransportSink;

#[cfg(feature = "message-queuing-telemetry-transport")]
impl OutputSink for MessageQueuingTelemetryTransportSink {
    fn publish(&mut self, _output: &TensorOutput) -> Result<(), OutputError> {
        todo!("MessageQueuingTelemetryTransportSink::publish is not implemented in the specification scaffold")
    }
    fn flush(&mut self) -> Result<(), OutputError> { Ok(()) }
    fn close(&mut self) -> Result<(), OutputError> { Ok(()) }
}

#[cfg(feature = "general-purpose-input-output-raspberry-pi")]
pub struct GeneralPurposeInputOutputSink { pub pin_number: u8, pub active_high: bool }

#[cfg(feature = "general-purpose-input-output-raspberry-pi")]
impl OutputSink for GeneralPurposeInputOutputSink {
    fn publish(&mut self, _output: &TensorOutput) -> Result<(), OutputError> {
        todo!("GeneralPurposeInputOutputSink::publish is not implemented in the specification scaffold")
    }
    fn flush(&mut self) -> Result<(), OutputError> { Ok(()) }
    fn close(&mut self) -> Result<(), OutputError> { Ok(()) }
}

pub mod register {
    pub fn register_all() {
        // Registration wiring placeholder.
    }
}
