//! (Work)bench [test] Graph implementation.
//!
//! This is the graph struct, and correspoding trait impls that are produced by the limen-build
//! graph builder for the following input, a concerete example has been given here for test purposes.
//!
//! ```
//! define_graph! {
//!     pub struct TestPipeline;
//!
//!     nodes {
//!         0: { ty: TestSourceNodeU32, in_ports: 0, out_ports: 1, in_payload: (),  out_payload: u32, name: Some("src") },
//!         1: { ty: TestIdentityModelNodeU32, in_ports: 1, out_ports: 1, in_payload: u32, out_payload: u32, name: Some("map") },
//!         2: { ty: TestSinkNodeU32,   in_ports: 1, out_ports: 0, in_payload: u32, out_payload: (),  name: Some("snk") }
//!     }
//!
//!     edges {
//!         0: { ty: Q32, payload: u32, from: (0,0), to: (1,0), policy: Q_32_POLICY, name: Some("e0") },
//!         1: { ty: Q32, payload: u32, from: (1,0), to: (2,0), policy: Q_32_POLICY, name: Some("e1") }
//!     }
//!
//!     wiring {
//!         node 0: { in: [ ],   out: [ 0 ] },
//!         node 1: { in: [ 0 ], out: [ 1 ] },
//!         node 2: { in: [ 1 ], out: [ ] }
//!     }
//! }
//! ```

use crate::{
    graph::{GraphApi, GraphEdgeAccess, GraphNodeAccess, GraphNodeContextBuilder, GraphNodeTypes},
    node::{
        bench::{TestIdentityModelNodeU32, TestSinkNodeU32, TestSourceNodeU32},
        StepContext,
    },
    policy::EdgePolicy,
    prelude::{EdgeDescriptor, EdgeLink, NodeDescriptor, NodeLink},
    queue::NoQueue,
    types::{EdgeIndex, NodeIndex, PortId, PortIndex},
};

type Q32 = crate::queue::bench::TestSpscRingBuf<crate::message::Message<u32>, 8>;

const Q_32_POLICY: EdgePolicy = EdgePolicy {
    caps: crate::policy::QueueCaps {
        max_items: 8,
        soft_items: 8,
        max_bytes: None,
        soft_bytes: None,
    },
    over_budget: crate::policy::OverBudgetAction::Drop,
    admission: crate::policy::AdmissionPolicy::DropOldest,
};

/// concrete graph implementation used for testing.
#[allow(clippy::complexity)]
pub struct TestPipeline {
    /// Nodes held in the graph.
    nodes: (
        NodeLink<TestSourceNodeU32, 0, 1, (), u32>,
        NodeLink<TestIdentityModelNodeU32, 1, 1, u32, u32>,
        NodeLink<TestSinkNodeU32, 1, 0, u32, ()>,
    ),
    /// Edges held in the graph.
    edges: (EdgeLink<Q32, u32>, EdgeLink<Q32, u32>),
}

impl TestPipeline {
    /// Returns a TestPipeline graph given the nodes and edges.
    #[inline]
    pub fn new(
        node_0: TestSourceNodeU32,
        node_1: TestIdentityModelNodeU32,
        node_2: TestSinkNodeU32,
        q_0: Q32,
        q_1: Q32,
    ) -> Self {
        let nodes = (
            NodeLink::<TestSourceNodeU32, 0, 1, (), u32>::new(
                node_0,
                NodeIndex::from(0usize),
                Some("src"),
            ),
            NodeLink::<TestIdentityModelNodeU32, 1, 1, u32, u32>::new(
                node_1,
                NodeIndex::from(1usize),
                Some("map"),
            ),
            NodeLink::<TestSinkNodeU32, 1, 0, u32, ()>::new(
                node_2,
                NodeIndex::from(2usize),
                Some("snk"),
            ),
        );

        let edges = (
            EdgeLink::<Q32, u32>::new(
                q_0,
                EdgeIndex::from(0usize),
                PortId {
                    node: NodeIndex::from(0usize),
                    port: PortIndex(0usize),
                },
                PortId {
                    node: NodeIndex::from(1usize),
                    port: PortIndex(0usize),
                },
                Q_32_POLICY,
                Some("e0"),
            ),
            EdgeLink::<Q32, u32>::new(
                q_1,
                EdgeIndex::from(1usize),
                PortId {
                    node: NodeIndex::from(1usize),
                    port: PortIndex(0usize),
                },
                PortId {
                    node: NodeIndex::from(2usize),
                    port: PortIndex(0usize),
                },
                Q_32_POLICY,
                Some("e1"),
            ),
        );

        Self { nodes, edges }
    }
}

// ===== GraphApi<3,2> =====
impl GraphApi<3, 2> for TestPipeline {
    #[inline]
    fn get_node_descriptors(&self) -> [NodeDescriptor; 3] {
        [
            self.nodes.0.descriptor(),
            self.nodes.1.descriptor(),
            self.nodes.2.descriptor(),
        ]
    }
    #[inline]
    fn get_edge_descriptors(&self) -> [EdgeDescriptor; 2] {
        [self.edges.0.descriptor(), self.edges.1.descriptor()]
    }
}

// ===== GraphNodeAccess<I> =====
impl GraphNodeAccess<0> for TestPipeline {
    type Node = NodeLink<TestSourceNodeU32, 0, 1, (), u32>;
    #[inline]
    fn node_ref(&self) -> &Self::Node {
        &self.nodes.0
    }
    #[inline]
    fn node_mut(&mut self) -> &mut Self::Node {
        &mut self.nodes.0
    }
}
impl GraphNodeAccess<1> for TestPipeline {
    type Node = NodeLink<TestIdentityModelNodeU32, 1, 1, u32, u32>;
    #[inline]
    fn node_ref(&self) -> &Self::Node {
        &self.nodes.1
    }
    #[inline]
    fn node_mut(&mut self) -> &mut Self::Node {
        &mut self.nodes.1
    }
}
impl GraphNodeAccess<2> for TestPipeline {
    type Node = NodeLink<TestSinkNodeU32, 1, 0, u32, ()>;
    #[inline]
    fn node_ref(&self) -> &Self::Node {
        &self.nodes.2
    }
    #[inline]
    fn node_mut(&mut self) -> &mut Self::Node {
        &mut self.nodes.2
    }
}

// ===== GraphEdgeAccess<E> =====
impl GraphEdgeAccess<0> for TestPipeline {
    type Edge = EdgeLink<Q32, u32>;
    #[inline]
    fn edge_ref(&self) -> &Self::Edge {
        &self.edges.0
    }
    #[inline]
    fn edge_mut(&mut self) -> &mut Self::Edge {
        &mut self.edges.0
    }
}
impl GraphEdgeAccess<1> for TestPipeline {
    type Edge = EdgeLink<Q32, u32>;
    #[inline]
    fn edge_ref(&self) -> &Self::Edge {
        &self.edges.1
    }
    #[inline]
    fn edge_mut(&mut self) -> &mut Self::Edge {
        &mut self.edges.1
    }
}

// ===== GraphNodeTypes<I, IN, OUT> =====
// node 0: IN=0, OUT=1
impl GraphNodeTypes<0, 0, 1> for TestPipeline {
    type InP = ();
    type OutP = u32;
    type InQ = NoQueue<()>;
    type OutQ = Q32;
}
// node 1: IN=1, OUT=1
impl GraphNodeTypes<1, 1, 1> for TestPipeline {
    type InP = u32;
    type OutP = u32;
    type InQ = Q32;
    type OutQ = Q32;
}
// node 2: IN=1, OUT=0
impl GraphNodeTypes<2, 1, 0> for TestPipeline {
    type InP = u32;
    type OutP = ();
    type InQ = Q32;
    type OutQ = NoQueue<()>;
}

// ===== GraphNodeContextBuilder<I, IN, OUT> =====
// node 0: in=[], out=[0]
impl GraphNodeContextBuilder<0, 0, 1> for TestPipeline {
    #[inline]
    fn make_step_context<'graph, 'telemetry, 'clock, C, T>(
        &'graph mut self,
        clock: &'clock C,
        telemetry: &'telemetry mut T,
    ) -> StepContext<
        'graph,
        'telemetry,
        'clock,
        0,
        1,
        <Self as GraphNodeTypes<0, 0, 1>>::InP,
        <Self as GraphNodeTypes<0, 0, 1>>::OutP,
        <Self as GraphNodeTypes<0, 0, 1>>::InQ,
        <Self as GraphNodeTypes<0, 0, 1>>::OutQ,
        C,
        T,
    >
    where
        EdgePolicy: Copy,
    {
        let out0_policy = self.edges.0.policy();

        let inputs: [&mut <Self as GraphNodeTypes<0, 0, 1>>::InQ; 0] = [/* empty */];
        let outputs: [&mut <Self as GraphNodeTypes<0, 0, 1>>::OutQ; 1] = [self.edges.0.queue_mut()];

        let in_policies: [EdgePolicy; 0] = [/* empty */];
        let out_policies: [EdgePolicy; 1] = [out0_policy];

        StepContext::<
            'graph,
            'telemetry,
            'clock,
            0,
            1,
            <Self as GraphNodeTypes<0, 0, 1>>::InP,
            <Self as GraphNodeTypes<0, 0, 1>>::OutP,
            <Self as GraphNodeTypes<0, 0, 1>>::InQ,
            <Self as GraphNodeTypes<0, 0, 1>>::OutQ,
            C,
            T,
        >::new(inputs, outputs, in_policies, out_policies, clock, telemetry)
    }
}

// node 1: in=[0], out=[1]
impl GraphNodeContextBuilder<1, 1, 1> for TestPipeline {
    #[inline]
    fn make_step_context<'graph, 'telemetry, 'clock, C, T>(
        &'graph mut self,
        clock: &'clock C,
        telemetry: &'telemetry mut T,
    ) -> StepContext<
        'graph,
        'telemetry,
        'clock,
        1,
        1,
        <Self as GraphNodeTypes<1, 1, 1>>::InP,
        <Self as GraphNodeTypes<1, 1, 1>>::OutP,
        <Self as GraphNodeTypes<1, 1, 1>>::InQ,
        <Self as GraphNodeTypes<1, 1, 1>>::OutQ,
        C,
        T,
    >
    where
        EdgePolicy: Copy,
    {
        let in0_policy = self.edges.0.policy();
        let out1_policy = self.edges.1.policy();

        let inputs: [&mut <Self as GraphNodeTypes<1, 1, 1>>::InQ; 1] = [self.edges.0.queue_mut()];
        let outputs: [&mut <Self as GraphNodeTypes<1, 1, 1>>::OutQ; 1] = [self.edges.1.queue_mut()];

        let in_policies: [EdgePolicy; 1] = [in0_policy];
        let out_policies: [EdgePolicy; 1] = [out1_policy];

        StepContext::<
            'graph,
            'telemetry,
            'clock,
            1,
            1,
            <Self as GraphNodeTypes<1, 1, 1>>::InP,
            <Self as GraphNodeTypes<1, 1, 1>>::OutP,
            <Self as GraphNodeTypes<1, 1, 1>>::InQ,
            <Self as GraphNodeTypes<1, 1, 1>>::OutQ,
            C,
            T,
        >::new(inputs, outputs, in_policies, out_policies, clock, telemetry)
    }
}

// node 2: in=[1], out=[]
impl GraphNodeContextBuilder<2, 1, 0> for TestPipeline {
    #[inline]
    fn make_step_context<'graph, 'telemetry, 'clock, C, T>(
        &'graph mut self,
        clock: &'clock C,
        telemetry: &'telemetry mut T,
    ) -> StepContext<
        'graph,
        'telemetry,
        'clock,
        1,
        0,
        <Self as GraphNodeTypes<2, 1, 0>>::InP,
        <Self as GraphNodeTypes<2, 1, 0>>::OutP,
        <Self as GraphNodeTypes<2, 1, 0>>::InQ,
        <Self as GraphNodeTypes<2, 1, 0>>::OutQ,
        C,
        T,
    >
    where
        EdgePolicy: Copy,
    {
        let in1_policy = self.edges.1.policy();

        let inputs: [&mut <Self as GraphNodeTypes<2, 1, 0>>::InQ; 1] = [self.edges.1.queue_mut()];
        let outputs: [&mut <Self as GraphNodeTypes<2, 1, 0>>::OutQ; 0] = [/* empty */];

        let in_policies: [EdgePolicy; 1] = [in1_policy];
        let out_policies: [EdgePolicy; 0] = [/* empty */];

        StepContext::<
            'graph,
            'telemetry,
            'clock,
            1,
            0,
            <Self as GraphNodeTypes<2, 1, 0>>::InP,
            <Self as GraphNodeTypes<2, 1, 0>>::OutP,
            <Self as GraphNodeTypes<2, 1, 0>>::InQ,
            <Self as GraphNodeTypes<2, 1, 0>>::OutQ,
            C,
            T,
        >::new(inputs, outputs, in_policies, out_policies, clock, telemetry)
    }
}
