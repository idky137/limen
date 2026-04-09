//! # Limen Pipeline Demo
//!
//! Demonstrates a **Sensor -> Model -> Actuator** inference pipeline built with
//! Limen's graph runtime. The same node logic and policies work identically on
//! bare-metal `no_std` targets and multi-threaded Linux servers.
//!
//! **What this demo shows:**
//!
//! 1. **Graph definition** via the `define_graph!` proc-macro DSL
//! 2. **Step-by-step execution** with edge occupancy visibility
//! 3. **Backpressure handling** — DropOldest policy evicting stale data
//! 4. **Batched inference** — fixed-size batch processing through the model node
//! 5. **Concurrent execution** — one thread per node with graceful shutdown
//! 6. **Structured telemetry** — per-node and per-edge metric collection
//!
//! Run with:
//! ```sh
//! cargo run -p limen-examples --features std --example pipeline_demo
//! ```

// ---------------------------------------------------------------------------
// Graph definition via proc-macro DSL
// ---------------------------------------------------------------------------
//
// This defines a three-node pipeline:
//
//   [Sensor]  --edge 0-->  [Model]  --edge 1-->  [Actuator]
//
// - Sensor:   produces incrementing tensor data (simulates a hardware sensor)
// - Model:    identity inference pass-through (simulates an ML model)
// - Actuator: consumes results (simulates an actuator or output device)
//
// Edges use SPSC (single-producer single-consumer) queues with DropOldest
// backpressure — when the queue is full, the oldest item is evicted to make
// room for fresh data. This is the correct policy for real-time systems where
// stale sensor readings should be discarded.

use limen_build::define_graph;
use limen_core::edge::Edge;
use limen_core::graph::{GraphApi, GraphNodeAccess};
use limen_core::memory::PlacementAcceptance;
use limen_core::message::MessageFlags;
use limen_core::node::bench::{
    TestCounterSourceTensor, TestIdentityModelNodeTensor, TestSinkNodeTensor, TestTensorBackend,
};
use limen_core::node::source::Source;
use limen_core::node::{Node, NodeCapabilities};
use limen_core::policy::{
    AdmissionPolicy, BatchingPolicy, BudgetPolicy, DeadlinePolicy, EdgePolicy, NodePolicy,
    OverBudgetAction, QueueCaps,
};
use limen_core::prelude::linux::NoStdLinuxMonotonicClock;
use limen_core::prelude::TestTensor;
use limen_core::runtime::LimenRuntime;
use limen_core::types::{QoSClass, SequenceNumber, TraceId};

// Concurrent imports (std only)
use limen_core::edge::spsc_concurrent::ConcurrentEdge;
use limen_core::memory::concurrent_manager::ConcurrentMemoryManager;
use limen_core::prelude::concurrent::{spawn_telemetry_core, TelemetrySender};
use limen_core::prelude::graph_telemetry::GraphTelemetry;
use limen_core::prelude::sink::IoLineWriter;
use limen_core::runtime::bench::concurrent_runtime::TestScopedRuntime;
use limen_core::telemetry::Telemetry;

// ---------------------------------------------------------------------------
// Edge policy: capacity 16, DropOldest backpressure, drop over-budget items.
// ---------------------------------------------------------------------------
const EDGE_POLICY: EdgePolicy = EdgePolicy::new(
    QueueCaps::new(16, 16, None, None),
    AdmissionPolicy::DropOldest,
    OverBudgetAction::Drop,
);

// ---------------------------------------------------------------------------
// No-std graph (step-by-step demo)
// ---------------------------------------------------------------------------
define_graph! {
    pub struct DemoPipelineNoStd;

    nodes {
        0: {
            ty: limen_core::node::bench::TestCounterSourceTensor<
                limen_core::prelude::linux::NoStdLinuxMonotonicClock, 32>,
            in_ports: 0,
            out_ports: 1,
            in_payload: (),
            out_payload: TestTensor,
            name: Some("sensor"),
            ingress_policy: EDGE_POLICY
        },
        1: {
            ty: limen_core::node::bench::TestIdentityModelNodeTensor<16>,
            in_ports: 1,
            out_ports: 1,
            in_payload: TestTensor,
            out_payload: TestTensor,
            name: Some("model")
        },
        2: {
            ty: limen_core::node::bench::TestSinkNodeTensor,
            in_ports: 1,
            out_ports: 0,
            in_payload: TestTensor,
            out_payload: (),
            name: Some("actuator")
        },
    }

    edges {
        0: {
            ty: limen_core::edge::bench::TestSpscRingBuf<16>,
            payload: TestTensor,
            manager: limen_core::memory::static_manager::StaticMemoryManager<TestTensor, 20>,
            from: (0, 0),
            to: (1, 0),
            policy: EDGE_POLICY,
            name: Some("sensor->model")
        },
        1: {
            ty: limen_core::edge::bench::TestSpscRingBuf<16>,
            payload: TestTensor,
            manager: limen_core::memory::static_manager::StaticMemoryManager<TestTensor, 20>,
            from: (1, 0),
            to: (2, 0),
            policy: EDGE_POLICY,
            name: Some("model->actuator")
        },
    }
}

// ---------------------------------------------------------------------------
// Concurrent graph (thread-per-node demo)
// ---------------------------------------------------------------------------
define_graph! {
    pub struct DemoPipelineConcurrent;

    nodes {
        0: {
            ty: limen_core::node::bench::TestCounterSourceTensor<
                limen_core::prelude::linux::NoStdLinuxMonotonicClock, 32>,
            in_ports: 0,
            out_ports: 1,
            in_payload: (),
            out_payload: TestTensor,
            name: Some("sensor"),
            ingress_policy: EDGE_POLICY
        },
        1: {
            ty: limen_core::node::bench::TestIdentityModelNodeTensor<16>,
            in_ports: 1,
            out_ports: 1,
            in_payload: TestTensor,
            out_payload: TestTensor,
            name: Some("model")
        },
        2: {
            ty: limen_core::node::bench::TestSinkNodeTensor,
            in_ports: 1,
            out_ports: 0,
            in_payload: TestTensor,
            out_payload: (),
            name: Some("actuator")
        },
    }

    edges {
        0: {
            ty: limen_core::edge::spsc_concurrent::ConcurrentEdge,
            payload: TestTensor,
            manager: limen_core::memory::concurrent_manager::ConcurrentMemoryManager<TestTensor>,
            from: (0, 0),
            to: (1, 0),
            policy: EDGE_POLICY,
            name: Some("sensor->model")
        },
        1: {
            ty: limen_core::edge::spsc_concurrent::ConcurrentEdge,
            payload: TestTensor,
            manager: limen_core::memory::concurrent_manager::ConcurrentMemoryManager<TestTensor>,
            from: (1, 0),
            to: (2, 0),
            policy: EDGE_POLICY,
            name: Some("model->actuator")
        },
    }

    concurrent;
}

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------
type Clock = NoStdLinuxMonotonicClock;
type Sensor = TestCounterSourceTensor<Clock, 32>;
type Model = TestIdentityModelNodeTensor<16>;
type Actuator = TestSinkNodeTensor;

type StdTelemetryInner = GraphTelemetry<3, 3, IoLineWriter<std::io::Stdout>>;
type StdTelemetry = TelemetrySender<StdTelemetryInner>;
type StdRuntime = TestScopedRuntime<Clock, StdTelemetry, 3, 3>;

fn main() {
    println!(
        "\n\
         ====================================================================\n\
         \x20 Limen Pipeline Demo -- Edge Inference Runtime\n\
         ====================================================================\n\
         \n\
         \x20 Graph topology:\n\
         \n\
         \x20   [Sensor] --edge 0--> [Model] --edge 1--> [Actuator]\n\
         \n\
         \x20 Edges:    SPSC queues, capacity 16, DropOldest backpressure\n\
         \x20 Batching: fixed size 4 on sensor and model nodes\n\
         \x20 Memory:   static (no-alloc) managers, 20 slots per edge\n"
    );

    part1_step_by_step();
    part2_concurrent();
}

// =========================================================================
// Part 1: Step-by-step execution (no_std compatible)
// =========================================================================
fn part1_step_by_step() {
    println!(
        "--------------------------------------------------------------------\n\
         \x20 Part 1: Step-by-step execution (no_std compatible)\n\
         --------------------------------------------------------------------\n"
    );

    let clock = Clock::new();

    // Node policies: fixed batch size of 4.
    let sensor_policy = NodePolicy::new(
        BatchingPolicy::fixed(4),
        BudgetPolicy::new(None, None),
        DeadlinePolicy::new(false, None, None),
    );
    let model_policy = NodePolicy::new(
        BatchingPolicy::fixed(4),
        BudgetPolicy::new(None, None),
        DeadlinePolicy::new(false, None, None),
    );
    let actuator_policy = NodePolicy::new(
        BatchingPolicy::none(),
        BudgetPolicy::new(None, None),
        DeadlinePolicy::new(false, None, None),
    );

    // Build nodes.
    let mut sensor: Sensor = Sensor::new(
        clock,
        0,
        TraceId::new(0u64),
        SequenceNumber::new(0u64),
        None,
        QoSClass::BestEffort,
        MessageFlags::empty(),
        NodeCapabilities::default(),
        sensor_policy,
        [PlacementAcceptance::default()],
        EDGE_POLICY,
    );

    // Populate the sensor's upstream backlog with 20 items.
    // This exceeds the edge capacity (16), so backpressure will engage.
    sensor.produce_n_items_in_backlog(20);
    println!("  Source backlog: 20 items loaded (exceeds edge capacity of 16)\n");

    let model: Model = Model::new(
        TestTensorBackend,
        (),
        model_policy,
        NodeCapabilities::default(),
        [PlacementAcceptance::default()],
        [PlacementAcceptance::default()],
    )
    .unwrap();

    let actuator: Actuator = Actuator::new(
        NodeCapabilities::default(),
        actuator_policy,
        [PlacementAcceptance::default()],
        |_s: &str| {
            // Suppress per-message output in step mode for cleaner demo output.
        },
    );

    // Build queues and memory managers (all static, no heap allocation).
    let q0 = limen_core::edge::bench::TestSpscRingBuf::<16>::default();
    let q1 = limen_core::edge::bench::TestSpscRingBuf::<16>::default();
    let mgr0 = limen_core::memory::static_manager::StaticMemoryManager::<TestTensor, 20>::new();
    let mgr1 = limen_core::memory::static_manager::StaticMemoryManager::<TestTensor, 20>::new();

    // Assemble the graph. All wiring is type-checked at compile time.
    let mut graph = DemoPipelineNoStd::new(sensor, model, actuator, q0, q1, mgr0, mgr1);

    // Validate graph integrity (descriptor consistency, edge wiring).
    graph.validate_graph().unwrap();

    // Create the no_std runtime (single-threaded, round-robin scheduler).
    use limen_core::prelude::graph_telemetry::GraphTelemetry;
    use limen_core::prelude::sink::FmtLineWriter;
    use limen_core::telemetry::sink::FixedBuffer;

    type NoStdTelemetry = GraphTelemetry<3, 3, FmtLineWriter<FixedBuffer<4096>>>;

    let telemetry_sink = FmtLineWriter::new(FixedBuffer::<4096>::new());
    let telemetry: NoStdTelemetry = NoStdTelemetry::new(0, true, telemetry_sink);

    use limen_core::runtime::bench::TestNoStdRuntime;

    type NoStdRuntime = TestNoStdRuntime<Clock, NoStdTelemetry, 3, 3>;
    type NoStdGraph = DemoPipelineNoStd;

    let mut runtime: NoStdRuntime = NoStdRuntime::new();
    runtime.init(&mut graph, clock, telemetry).unwrap();

    // Step through the pipeline, printing edge occupancies after each step.
    let total_steps = 15;
    println!(
        "  {:>6} | {:>16} | {:>20}",
        "Step", "sensor->model", "model->actuator"
    );
    println!("  {:-<6}-+-{:-<16}-+-{:-<20}", "", "", "");

    for step in 1..=total_steps {
        let _ = runtime.step(&mut graph).unwrap();

        let occ = LimenRuntime::<NoStdGraph, 3, 3>::occupancies(&runtime);
        // occ[0] is the virtual ingress edge, occ[1] is sensor->model, occ[2] is model->actuator
        println!(
            "  {:>6} | {:>10} items | {:>14} items",
            step,
            occ[1].items(),
            occ[2].items(),
        );
    }

    // Access the actuator to check processed count.
    let actuator_node = <DemoPipelineNoStd as GraphNodeAccess<2>>::node_mut(&mut graph);
    let processed = *actuator_node.node_mut().sink_mut().processed();

    println!("\n  Pipeline complete:");
    println!("    Messages consumed by actuator: {}", processed);
    println!("    Backpressure policy: DropOldest (stale data evicted)\n");
}

// =========================================================================
// Part 2: Concurrent execution (std, thread-per-node)
// =========================================================================
fn part2_concurrent() {
    println!(
        "--------------------------------------------------------------------\n\
         \x20 Part 2: Concurrent execution (std, thread-per-node)\n\
         --------------------------------------------------------------------\n"
    );

    let clock = Clock::new();

    let sensor_policy = NodePolicy::new(
        BatchingPolicy::fixed(4),
        BudgetPolicy::new(None, None),
        DeadlinePolicy::new(false, None, None),
    );
    let model_policy = NodePolicy::new(
        BatchingPolicy::fixed(4),
        BudgetPolicy::new(None, None),
        DeadlinePolicy::new(false, None, None),
    );
    let actuator_policy = NodePolicy::new(
        BatchingPolicy::none(),
        BudgetPolicy::new(None, None),
        DeadlinePolicy::new(false, None, None),
    );

    let mut sensor: Sensor = Sensor::new(
        clock,
        0,
        TraceId::new(0u64),
        SequenceNumber::new(0u64),
        None,
        QoSClass::BestEffort,
        MessageFlags::empty(),
        NodeCapabilities::default(),
        sensor_policy,
        [PlacementAcceptance::default()],
        EDGE_POLICY,
    );
    sensor.produce_n_items_in_backlog(20);

    let model: Model = Model::new(
        TestTensorBackend,
        (),
        model_policy,
        NodeCapabilities::default(),
        [PlacementAcceptance::default()],
        [PlacementAcceptance::default()],
    )
    .unwrap();

    let actuator: Actuator = Actuator::new(
        NodeCapabilities::default(),
        actuator_policy,
        [PlacementAcceptance::default()],
        |_s: &str| {
            // Suppress per-message output for clean demo.
        },
    );

    // Concurrent edges and heap-backed memory managers.
    let q0 = ConcurrentEdge::new(16);
    let q1 = ConcurrentEdge::new(16);
    let mgr0 = ConcurrentMemoryManager::<TestTensor>::new(20);
    let mgr1 = ConcurrentMemoryManager::<TestTensor>::new(20);

    let mut graph = DemoPipelineConcurrent::new(sensor, model, actuator, q0, q1, mgr0, mgr1);
    graph.validate_graph().unwrap();

    // Telemetry: GraphTelemetry writing to stdout, wrapped in a concurrent sender.
    // Events are disabled during the run to keep output clean; only the final
    // metrics summary is printed.
    let sink = IoLineWriter::<std::io::Stdout>::stdout_writer();
    let inner_telemetry: StdTelemetryInner = StdTelemetryInner::new(0, false, sink);
    let telemetry_core = spawn_telemetry_core(inner_telemetry);
    let telemetry: StdTelemetry = telemetry_core.sender();

    // Runtime: one worker thread per node, simple backoff scheduler.
    let mut runtime: StdRuntime = StdRuntime::new();
    runtime.init(&mut graph, clock, telemetry).unwrap();

    // Get a stop handle (thread-safe), then launch concurrent execution.
    type ConcurrentGraph = DemoPipelineConcurrent;

    let run_duration = std::time::Duration::from_millis(500);
    let handle = LimenRuntime::<ConcurrentGraph, 3, 3>::stop_handle(&runtime).unwrap();

    println!("  Launching 3 worker threads (one per node)...");
    println!(
        "  Running for {}ms with continuous sensor input.\n",
        run_duration.as_millis()
    );

    std::thread::spawn(move || {
        std::thread::sleep(run_duration);
        handle.request_stop();
    });

    LimenRuntime::<ConcurrentGraph, 3, 3>::run(&mut runtime, &mut graph).unwrap();
    graph.validate_graph().unwrap();

    // Read processed count from the actuator.
    let actuator_node = <DemoPipelineConcurrent as GraphNodeAccess<2>>::node_mut(&mut graph);
    let processed = *actuator_node.node_mut().sink_mut().processed();

    println!("  Concurrent pipeline stopped gracefully via RuntimeStopHandle.");
    println!("  Messages processed by actuator: {}\n", processed);

    // Push final telemetry snapshot and flush.
    println!(
        "--------------------------------------------------------------------\n\
         \x20 Telemetry Summary\n\
         --------------------------------------------------------------------\n"
    );

    runtime
        .with_telemetry(|telemetry| {
            telemetry.push_metrics();
            telemetry.flush();
        })
        .expect("telemetry flush failed");

    telemetry_core.shutdown_and_join();

    println!(
        "\n\
         ====================================================================\n\
         \x20 Demo complete. For more information:\n\
         \n\
         \x20   Quickstart guide:     docs/quickstart.md\n\
         \x20   Architecture guide:   docs/architecture/index.md\n\
         \x20   API reference:        cargo doc --workspace --open\n\
         ====================================================================\n"
    );
}
