#![doc = r#"
limen-platform

Platform backends exposing constraints and lifecycle hooks.
"#]

use limen_core::errors::RuntimeError;
use limen_core::traits::{PlatformBackend, PlatformConstraints};

pub struct DesktopPlatformBackend;

impl PlatformBackend for DesktopPlatformBackend {
    fn description(&self) -> &'static str { "desktop-linux" }
    fn constraints(&self) -> PlatformConstraints {
        PlatformConstraints {
            maximum_memory_bytes: None,
            general_purpose_input_output_available: false,
            preferred_thread_count: None,
        }
    }
    fn initialize(&mut self) -> Result<(), RuntimeError> { Ok(()) }
    fn shutdown(&mut self) -> Result<(), RuntimeError> { Ok(()) }
}

pub struct RaspberryPiLinuxPlatformBackend;

impl PlatformBackend for RaspberryPiLinuxPlatformBackend {
    fn description(&self) -> &'static str { "raspberry-pi-linux" }
    fn constraints(&self) -> PlatformConstraints {
        PlatformConstraints {
            maximum_memory_bytes: None,
            general_purpose_input_output_available: true,
            preferred_thread_count: Some(4),
        }
    }
    fn initialize(&mut self) -> Result<(), RuntimeError> { Ok(()) }
    fn shutdown(&mut self) -> Result<(), RuntimeError> { Ok(()) }
}

pub struct JetsonLinuxPlatformBackend;

impl PlatformBackend for JetsonLinuxPlatformBackend {
    fn description(&self) -> &'static str { "jetson-linux" }
    fn constraints(&self) -> PlatformConstraints {
        PlatformConstraints {
            maximum_memory_bytes: None,
            general_purpose_input_output_available: false,
            preferred_thread_count: Some(6),
        }
    }
    fn initialize(&mut self) -> Result<(), RuntimeError> { Ok(()) }
    fn shutdown(&mut self) -> Result<(), RuntimeError> { Ok(()) }
}

pub mod register {
    pub fn register_all() {
        // Registration wiring placeholder.
    }
}
