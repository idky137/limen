#![doc = r#"
limen

Dynamic runtime with configuration-driven orchestration, registry-based factory
wiring, and a CLI to launch pipelines. MVP supports a linear pipeline only.
"#]

pub mod config;
pub mod registry;
pub mod builder;
pub mod observability_init;
