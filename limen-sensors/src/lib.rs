#![doc = r#"
limen-sensors

Sensor stream implementations producing `SensorData`. Implementations are
non-blocking and must return `Ok(None)` promptly when no data is available.
"#]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use limen_core::errors::SensorError;
use limen_core::traits::SensorStream;
use limen_core::types::{SensorData, SensorMetadata, SequenceNumber, Timestamp};

pub struct CommaSeparatedValuesSensor;

impl SensorStream for CommaSeparatedValuesSensor {
    fn open(&mut self) -> Result<(), SensorError> { Ok(()) }
    fn read_next(&mut self) -> Result<Option<SensorData>, SensorError> {
        todo!("CommaSeparatedValuesSensor::read_next is not implemented in the specification scaffold")
    }
    fn close(&mut self) -> Result<(), SensorError> { Ok(()) }
    fn reset(&mut self) -> Result<(), SensorError> { Ok(()) }
}

pub struct SimulatedSensor;

impl SensorStream for SimulatedSensor {
    fn open(&mut self) -> Result<(), SensorError> { Ok(()) }
    fn read_next(&mut self) -> Result<Option<SensorData>, SensorError> {
        todo!("SimulatedSensor::read_next is not implemented in the specification scaffold")
    }
    fn close(&mut self) -> Result<(), SensorError> { Ok(()) }
    fn reset(&mut self) -> Result<(), SensorError> { Ok(()) }
}

#[cfg(feature = "message-queuing-telemetry-transport")]
pub struct MessageQueuingTelemetryTransportSensor;

#[cfg(feature = "message-queuing-telemetry-transport")]
impl SensorStream for MessageQueuingTelemetryTransportSensor {
    fn open(&mut self) -> Result<(), SensorError> { Ok(()) }
    fn read_next(&mut self) -> Result<Option<SensorData>, SensorError> {
        todo!("MessageQueuingTelemetryTransportSensor::read_next is not implemented in the specification scaffold")
    }
    fn close(&mut self) -> Result<(), SensorError> { Ok(()) }
    fn reset(&mut self) -> Result<(), SensorError> { Ok(()) }
}

#[cfg(feature = "serial-port")]
pub struct SerialPortSensor;

#[cfg(feature = "serial-port")]
impl SensorStream for SerialPortSensor {
    fn open(&mut self) -> Result<(), SensorError> { Ok(()) }
    fn read_next(&mut self) -> Result<Option<SensorData>, SensorError> {
        todo!("SerialPortSensor::read_next is not implemented in the specification scaffold")
    }
    fn close(&mut self) -> Result<(), SensorError> { Ok(()) }
    fn reset(&mut self) -> Result<(), SensorError> { Ok(()) }
}

#[cfg(feature = "audio-linux")]
pub struct PulseCodeModulationAudioSensor;

#[cfg(feature = "audio-linux")]
impl SensorStream for PulseCodeModulationAudioSensor {
    fn open(&mut self) -> Result<(), SensorError> { Ok(()) }
    fn read_next(&mut self) -> Result<Option<SensorData>, SensorError> {
        todo!("PulseCodeModulationAudioSensor::read_next is not implemented in the specification scaffold")
    }
    fn close(&mut self) -> Result<(), SensorError> { Ok(()) }
    fn reset(&mut self) -> Result<(), SensorError> { Ok(()) }
}

pub mod register {
    pub fn register_all() {
        // Registration wiring placeholder.
    }
}
