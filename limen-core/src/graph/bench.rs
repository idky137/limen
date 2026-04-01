//! (Work)bench [test] Graph implementations.
//!
//! These are the graph structs, and correspoding trait impls that are produced by the limen-build
//! graph builder for the following input, a concerete example has been given here for test purposes.
//!
//! ```text
//! define_graph! {
//!     pub struct TestPipeline;
//!
//!     nodes {
//!         0: { ty: TestCounterSourceU32, in_ports: 0, out_ports: 1, in_payload: (),  out_payload: u32, name: Some("src"), ingress_policy: Q_32_POLICY },
//!         1: { ty: TestIdentityModelNodeU32, in_ports: 1, out_ports: 1, in_payload: u32, out_payload: u32, name: Some("map") },
//!         2: { ty: TestSinkNodeU32,   in_ports: 1, out_ports: 0, in_payload: u32, out_payload: (),  name: Some("snk") }
//!     }
//!
//!     edges {
//!         0: { ty: Q32, payload: u32, manager: StaticMemoryManager<u32, 8>, from: (0,0), to: (1,0), policy: Q_32_POLICY, name: Some("e0") },
//!         1: { ty: Q32, payload: u32, manager: StaticMemoryManager<u32, 8>, from: (1,0), to: (2,0), policy: Q_32_POLICY, name: Some("e1") }
//!     }
//! }
//! ```

use crate::{
    edge::{Edge as _, EdgeOccupancy, NoQueue},
    errors::{GraphError, NodeError},
    graph::{GraphApi, GraphEdgeAccess, GraphNodeAccess, GraphNodeContextBuilder, GraphNodeTypes},
    node::{
        bench::{TestCounterSourceU32_2, TestIdentityModelNodeU32_2, TestSinkNodeU32_2},
        sink::SinkNode,
        source::{Source as _, SourceNode, EXTERNAL_INGRESS_NODE},
        Node as _, StepContext, StepResult,
    },
    policy::{AdmissionPolicy, EdgePolicy, NodePolicy, OverBudgetAction},
    prelude::{
        EdgeDescriptor, EdgeLink, NodeDescriptor, NodeLink, PlatformClock, StaticMemoryManager,
        Telemetry,
    },
    types::{EdgeIndex, NodeIndex, PortId, PortIndex},
};

// Test edge types.
type Q32 = crate::edge::bench::TestSpscRingBuf<8>;
const Q_32_POLICY: EdgePolicy = EdgePolicy {
    caps: crate::policy::QueueCaps {
        max_items: 8,
        soft_items: 8,
        max_bytes: None,
        soft_bytes: None,
    },
    over_budget: OverBudgetAction::Drop,
    admission: AdmissionPolicy::DropOldest,
};

// Test memory manager type (one per real edge).
type Mgr32 = StaticMemoryManager<u32, 8>;

// Test source node types.
#[allow(type_alias_bounds)]
type SrcNode<SrcClk: PlatformClock> = SourceNode<TestCounterSourceU32_2<SrcClk, 32>, u32, 1>;
const INGRESS_POLICY: EdgePolicy = Q_32_POLICY;

// Test model node types.
const TEST_MAX_BATCH: usize = 32;
type MapNode = TestIdentityModelNodeU32_2<TEST_MAX_BATCH>;

// Test sink node types.
type SnkNode = SinkNode<TestSinkNodeU32_2, u32, 1>;

/// concrete graph implementation used for testing.
#[allow(clippy::complexity)]
pub struct TestPipeline<SrcClk: PlatformClock> {
    /// Nodes held in the graph.
    nodes: (
        NodeLink<SrcNode<SrcClk>, 0, 1, (), u32>,
        NodeLink<MapNode, 1, 1, u32, u32>,
        NodeLink<SnkNode, 1, 0, u32, ()>,
    ),
    /// Edges held in the graph.
    edges: (EdgeLink<Q32>, EdgeLink<Q32>),
    /// Memory managers for all *real* edges in declaration order.
    managers: (Mgr32, Mgr32),
}

impl<SrcClk: PlatformClock> TestPipeline<SrcClk> {
    /// Returns a TestPipeline graph given the nodes and edges.
    #[inline]
    pub fn new(
        node_0: impl Into<SrcNode<SrcClk>>,
        node_1: MapNode,
        node_2: impl Into<SnkNode>,
        q_0: Q32,
        q_1: Q32,
        mgr_0: Mgr32,
        mgr_1: Mgr32,
    ) -> Self {
        let node_0: SrcNode<SrcClk> = node_0.into();
        let node_2: SnkNode = node_2.into();

        let nodes = (
            NodeLink::<SrcNode<SrcClk>, 0, 1, (), u32>::new(
                node_0,
                NodeIndex::from(0usize),
                Some("src"),
            ),
            NodeLink::<MapNode, 1, 1, u32, u32>::new(node_1, NodeIndex::from(1usize), Some("map")),
            NodeLink::<SnkNode, 1, 0, u32, ()>::new(node_2, NodeIndex::from(2usize), Some("snk")),
        );

        let edges = (
            EdgeLink::<Q32>::new(
                q_0,
                EdgeIndex::from(1usize),
                PortId::new(NodeIndex::from(0usize), PortIndex::from(0usize)),
                PortId::new(NodeIndex::from(1usize), PortIndex::from(0usize)),
                Q_32_POLICY,
                Some("e0"),
            ),
            EdgeLink::<Q32>::new(
                q_1,
                EdgeIndex::from(2usize),
                PortId::new(NodeIndex::from(1usize), PortIndex::from(0usize)),
                PortId::new(NodeIndex::from(2usize), PortIndex::from(0usize)),
                Q_32_POLICY,
                Some("e1"),
            ),
        );

        let managers = (mgr_0, mgr_1);

        Self {
            nodes,
            edges,
            managers,
        }
    }
}

// ===== GraphApi<3,3> =====
impl<SrcClk: PlatformClock> GraphApi<3, 3> for TestPipeline<SrcClk> {
    #[inline]
    fn get_node_descriptors(&self) -> [NodeDescriptor; 3] {
        [
            self.nodes.0.descriptor(),
            self.nodes.1.descriptor(),
            self.nodes.2.descriptor(),
        ]
    }
    #[inline]
    fn get_edge_descriptors(&self) -> [EdgeDescriptor; 3] {
        [
            EdgeDescriptor::new(
                EdgeIndex::from(0usize),
                PortId::new(EXTERNAL_INGRESS_NODE, PortIndex::from(0usize)),
                PortId::new(NodeIndex::from(0usize), PortIndex::from(0usize)),
                Some("ingress0"),
            ),
            self.edges.0.descriptor(),
            self.edges.1.descriptor(),
        ]
    }

    #[inline]
    fn get_node_policies(&self) -> [NodePolicy; 3] {
        [
            self.nodes.0.policy(),
            self.nodes.1.policy(),
            self.nodes.2.policy(),
        ]
    }

    #[inline]
    fn get_edge_policies(&self) -> [EdgePolicy; 3] {
        [
            INGRESS_POLICY,
            *self.edges.0.policy(),
            *self.edges.1.policy(),
        ]
    }

    #[inline]
    fn edge_occupancy_for<const E: usize>(&self) -> Result<EdgeOccupancy, GraphError> {
        let occ = match E {
            0 => {
                let src = self.nodes.0.node().source_ref();
                src.ingress_occupancy()
            }
            1 => {
                let e = &self.edges.0;
                e.occupancy(e.policy())
            }
            2 => {
                let e = &self.edges.1;
                e.occupancy(e.policy())
            }
            _ => return Err(GraphError::InvalidEdgeIndex), // use your variant
        };
        Ok(occ)
    }

    #[inline]
    fn write_all_edge_occupancies(&self, out: &mut [EdgeOccupancy; 3]) -> Result<(), GraphError> {
        out[0] = self.edge_occupancy_for::<0>()?;
        out[1] = self.edge_occupancy_for::<1>()?;
        out[2] = self.edge_occupancy_for::<2>()?;
        Ok(())
    }

    #[inline]
    fn refresh_occupancies_for_node<const I: usize, const IN: usize, const OUT: usize>(
        &self,
        out: &mut [EdgeOccupancy; 3],
    ) -> Result<(), GraphError> {
        let node_idx = NodeIndex::from(I);
        // Iterate *all* edges; update those where this node is upstream OR downstream.
        for ed in self.get_edge_descriptors().iter() {
            if ed.upstream().node() == &node_idx || ed.downstream().node() == &node_idx {
                let ei = (ed.id()).as_usize();
                match ei {
                    0 => {
                        out[0] = self.edge_occupancy_for::<0>()?;
                    }
                    1 => {
                        out[1] = self.edge_occupancy_for::<1>()?;
                    }
                    2 => {
                        out[2] = self.edge_occupancy_for::<2>()?;
                    }
                    _ => return Err(GraphError::InvalidEdgeIndex),
                }
            }
        }
        Ok(())
    }

    #[inline]
    fn step_node_by_index<C, T>(
        &mut self,
        index: usize,
        clock: &C,
        telemetry: &mut T,
    ) -> Result<StepResult, NodeError>
    where
        EdgePolicy: Copy,
        C: PlatformClock + Sized,
        T: Telemetry + Sized,
    {
        match index {
            0 => <Self as GraphNodeContextBuilder<0, 0, 1>>::with_node_and_step_context::<
                C,
                T,
                StepResult,
                NodeError,
            >(self, clock, telemetry, |node, ctx| node.step(ctx)),

            1 => <Self as GraphNodeContextBuilder<1, 1, 1>>::with_node_and_step_context::<
                C,
                T,
                StepResult,
                NodeError,
            >(self, clock, telemetry, |node, ctx| node.step(ctx)),

            2 => <Self as GraphNodeContextBuilder<2, 1, 0>>::with_node_and_step_context::<
                C,
                T,
                StepResult,
                NodeError,
            >(self, clock, telemetry, |node, ctx| node.step(ctx)),

            _ => unreachable!("invalid node index"),
        }
    }
}

// ===== GraphNodeAccess<I> =====
impl<SrcClk: PlatformClock> GraphNodeAccess<0> for TestPipeline<SrcClk> {
    type Node = NodeLink<SrcNode<SrcClk>, 0, 1, (), u32>;
    #[inline]
    fn node_ref(&self) -> &Self::Node {
        &self.nodes.0
    }
    #[inline]
    fn node_mut(&mut self) -> &mut Self::Node {
        &mut self.nodes.0
    }
}
impl<SrcClk: PlatformClock> GraphNodeAccess<1> for TestPipeline<SrcClk> {
    type Node = NodeLink<MapNode, 1, 1, u32, u32>;
    #[inline]
    fn node_ref(&self) -> &Self::Node {
        &self.nodes.1
    }
    #[inline]
    fn node_mut(&mut self) -> &mut Self::Node {
        &mut self.nodes.1
    }
}
impl<SrcClk: PlatformClock> GraphNodeAccess<2> for TestPipeline<SrcClk> {
    type Node = NodeLink<SnkNode, 1, 0, u32, ()>;
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
impl<SrcClk: PlatformClock> GraphEdgeAccess<1> for TestPipeline<SrcClk> {
    type Edge = EdgeLink<Q32>;
    #[inline]
    fn edge_ref(&self) -> &Self::Edge {
        &self.edges.0
    }
    #[inline]
    fn edge_mut(&mut self) -> &mut Self::Edge {
        &mut self.edges.0
    }
}
impl<SrcClk: PlatformClock> GraphEdgeAccess<2> for TestPipeline<SrcClk> {
    type Edge = EdgeLink<Q32>;
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
impl<SrcClk: PlatformClock> GraphNodeTypes<0, 0, 1> for TestPipeline<SrcClk> {
    type InP = ();
    type OutP = u32;
    type InQ = NoQueue;
    type OutQ = Q32;
    type InM = StaticMemoryManager<(), 1>;
    type OutM = Mgr32;
}
// node 1: IN=1, OUT=1
impl<SrcClk: PlatformClock> GraphNodeTypes<1, 1, 1> for TestPipeline<SrcClk> {
    type InP = u32;
    type OutP = u32;
    type InQ = Q32;
    type OutQ = Q32;
    type InM = Mgr32;
    type OutM = Mgr32;
}
// node 2: IN=1, OUT=0
impl<SrcClk: PlatformClock> GraphNodeTypes<2, 1, 0> for TestPipeline<SrcClk> {
    type InP = u32;
    type OutP = ();
    type InQ = Q32;
    type OutQ = NoQueue;
    type InM = Mgr32;
    type OutM = StaticMemoryManager<(), 1>;
}

// ===== GraphNodeContextBuilder<I, IN, OUT> =====
// node 0: in=[], out=[edge id 1]
impl<SrcClk: PlatformClock> GraphNodeContextBuilder<0, 0, 1> for TestPipeline<SrcClk>
where
    Self: GraphNodeAccess<0, Node = NodeLink<SrcNode<SrcClk>, 0, 1, (), u32>>,
{
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
        <Self as GraphNodeTypes<0, 0, 1>>::InM,
        <Self as GraphNodeTypes<0, 0, 1>>::OutM,
        C,
        T,
    >
    where
        EdgePolicy: Copy,
        C: PlatformClock + Sized,
        T: Telemetry + Sized,
    {
        let out0_policy = *self.edges.0.policy();

        let inputs: [&mut <Self as GraphNodeTypes<0, 0, 1>>::InQ; 0] = [/* empty */];
        let outputs: [&mut <Self as GraphNodeTypes<0, 0, 1>>::OutQ; 1] = [self.edges.0.queue_mut()];

        let in_managers: [&'graph mut <Self as GraphNodeTypes<0, 0, 1>>::InM; 0] = [];
        let out_managers: [&'graph mut <Self as GraphNodeTypes<0, 0, 1>>::OutM; 1] =
            [&mut self.managers.0];

        let in_policies: [EdgePolicy; 0] = [/* empty */];
        let out_policies: [EdgePolicy; 1] = [out0_policy];

        let node_id: u32 = 0;
        let in_edge_ids: [u32; 0] = [/* empty */];
        let out_edge_ids: [u32; 1] = [1];

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
            <Self as GraphNodeTypes<0, 0, 1>>::InM,
            <Self as GraphNodeTypes<0, 0, 1>>::OutM,
            C,
            T,
        >::new(
            inputs,
            outputs,
            in_managers,
            out_managers,
            in_policies,
            out_policies,
            node_id,
            in_edge_ids,
            out_edge_ids,
            clock,
            telemetry,
        )
    }

    #[inline]
    fn with_node_and_step_context<'telemetry, 'clock, C, T, R, E>(
        &mut self,
        clock: &'clock C,
        telemetry: &'telemetry mut T,
        f: impl FnOnce(
            &mut <Self as GraphNodeAccess<0>>::Node,
            &mut StepContext<
                '_,
                'telemetry,
                'clock,
                0,
                1,
                <Self as GraphNodeTypes<0, 0, 1>>::InP,
                <Self as GraphNodeTypes<0, 0, 1>>::OutP,
                <Self as GraphNodeTypes<0, 0, 1>>::InQ,
                <Self as GraphNodeTypes<0, 0, 1>>::OutQ,
                <Self as GraphNodeTypes<0, 0, 1>>::InM,
                <Self as GraphNodeTypes<0, 0, 1>>::OutM,
                C,
                T,
            >,
        ) -> Result<R, E>,
    ) -> Result<R, E>
    where
        Self: GraphNodeAccess<0>,
        EdgePolicy: Copy,
        C: PlatformClock + Sized,
        T: Telemetry + Sized,
    {
        let node = &mut self.nodes.0;

        let out0_policy = *self.edges.0.policy();

        let inputs: [&mut <Self as GraphNodeTypes<0, 0, 1>>::InQ; 0] = [/* empty */];
        let outputs: [&mut <Self as GraphNodeTypes<0, 0, 1>>::OutQ; 1] = [self.edges.0.queue_mut()];

        let in_managers: [&mut <Self as GraphNodeTypes<0, 0, 1>>::InM; 0] = [/* empty */];
        let out_managers: [&mut <Self as GraphNodeTypes<0, 0, 1>>::OutM; 1] =
            [&mut self.managers.0];

        let in_policies: [EdgePolicy; 0] = [/* empty */];
        let out_policies: [EdgePolicy; 1] = [out0_policy];

        let node_id: u32 = 0;
        let in_edge_ids: [u32; 0] = [/* empty */];
        let out_edge_ids: [u32; 1] = [1];

        let mut ctx = StepContext::new(
            inputs,
            outputs,
            in_managers,
            out_managers,
            in_policies,
            out_policies,
            node_id,
            in_edge_ids,
            out_edge_ids,
            clock,
            telemetry,
        );
        f(node, &mut ctx)
    }
}

// node 1: in=[edge id 1], out=[edge id 2]
impl<SrcClk: PlatformClock> GraphNodeContextBuilder<1, 1, 1> for TestPipeline<SrcClk>
where
    Self: GraphNodeAccess<1, Node = NodeLink<MapNode, 1, 1, u32, u32>>,
{
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
        <Self as GraphNodeTypes<1, 1, 1>>::InM,
        <Self as GraphNodeTypes<1, 1, 1>>::OutM,
        C,
        T,
    >
    where
        EdgePolicy: Copy,
        C: PlatformClock + Sized,
        T: Telemetry + Sized,
    {
        let in0_policy = *self.edges.0.policy();
        let out1_policy = *self.edges.1.policy();

        let inputs: [&mut <Self as GraphNodeTypes<1, 1, 1>>::InQ; 1] = [self.edges.0.queue_mut()];
        let outputs: [&mut <Self as GraphNodeTypes<1, 1, 1>>::OutQ; 1] = [self.edges.1.queue_mut()];

        let in_managers: [&'graph mut <Self as GraphNodeTypes<1, 1, 1>>::InM; 1] =
            [&mut self.managers.0];
        let out_managers: [&'graph mut <Self as GraphNodeTypes<1, 1, 1>>::OutM; 1] =
            [&mut self.managers.1];

        let in_policies: [EdgePolicy; 1] = [in0_policy];
        let out_policies: [EdgePolicy; 1] = [out1_policy];

        let node_id: u32 = 1;
        let in_edge_ids: [u32; 1] = [1];
        let out_edge_ids: [u32; 1] = [2];

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
            <Self as GraphNodeTypes<1, 1, 1>>::InM,
            <Self as GraphNodeTypes<1, 1, 1>>::OutM,
            C,
            T,
        >::new(
            inputs,
            outputs,
            in_managers,
            out_managers,
            in_policies,
            out_policies,
            node_id,
            in_edge_ids,
            out_edge_ids,
            clock,
            telemetry,
        )
    }

    #[inline]
    fn with_node_and_step_context<'telemetry, 'clock, C, T, R, E>(
        &mut self,
        clock: &'clock C,
        telemetry: &'telemetry mut T,
        f: impl FnOnce(
            &mut <Self as GraphNodeAccess<1>>::Node,
            &mut StepContext<
                '_,
                'telemetry,
                'clock,
                1,
                1,
                <Self as GraphNodeTypes<1, 1, 1>>::InP,
                <Self as GraphNodeTypes<1, 1, 1>>::OutP,
                <Self as GraphNodeTypes<1, 1, 1>>::InQ,
                <Self as GraphNodeTypes<1, 1, 1>>::OutQ,
                <Self as GraphNodeTypes<1, 1, 1>>::InM,
                <Self as GraphNodeTypes<1, 1, 1>>::OutM,
                C,
                T,
            >,
        ) -> Result<R, E>,
    ) -> Result<R, E>
    where
        Self: GraphNodeAccess<1>,
        EdgePolicy: Copy,
        C: PlatformClock + Sized,
        T: Telemetry + Sized,
    {
        let node = &mut self.nodes.1;

        let in0_policy = *self.edges.0.policy();
        let out1_policy = *self.edges.1.policy();

        let inputs: [&mut <Self as GraphNodeTypes<1, 1, 1>>::InQ; 1] = [self.edges.0.queue_mut()];
        let outputs: [&mut <Self as GraphNodeTypes<1, 1, 1>>::OutQ; 1] = [self.edges.1.queue_mut()];

        let in_managers: [&mut <Self as GraphNodeTypes<1, 1, 1>>::InM; 1] = [&mut self.managers.0];
        let out_managers: [&mut <Self as GraphNodeTypes<1, 1, 1>>::OutM; 1] =
            [&mut self.managers.1];

        let in_policies: [EdgePolicy; 1] = [in0_policy];
        let out_policies: [EdgePolicy; 1] = [out1_policy];

        let node_id: u32 = 1;
        let in_edge_ids: [u32; 1] = [1];
        let out_edge_ids: [u32; 1] = [2];

        let mut ctx = StepContext::new(
            inputs,
            outputs,
            in_managers,
            out_managers,
            in_policies,
            out_policies,
            node_id,
            in_edge_ids,
            out_edge_ids,
            clock,
            telemetry,
        );
        f(node, &mut ctx)
    }
}

// node 2: in=[edge id 2], out=[]
impl<SrcClk: PlatformClock> GraphNodeContextBuilder<2, 1, 0> for TestPipeline<SrcClk>
where
    Self: GraphNodeAccess<2, Node = NodeLink<SnkNode, 1, 0, u32, ()>>,
{
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
        <Self as GraphNodeTypes<2, 1, 0>>::InM,
        <Self as GraphNodeTypes<2, 1, 0>>::OutM,
        C,
        T,
    >
    where
        EdgePolicy: Copy,
        C: PlatformClock + Sized,
        T: Telemetry + Sized,
    {
        let in1_policy = *self.edges.1.policy();

        let inputs: [&mut <Self as GraphNodeTypes<2, 1, 0>>::InQ; 1] = [self.edges.1.queue_mut()];
        let outputs: [&mut <Self as GraphNodeTypes<2, 1, 0>>::OutQ; 0] = [/* empty */];

        let in_managers: [&'graph mut <Self as GraphNodeTypes<2, 1, 0>>::InM; 1] =
            [&mut self.managers.1];
        let out_managers: [&'graph mut <Self as GraphNodeTypes<2, 1, 0>>::OutM; 0] = [/* empty */];

        let in_policies: [EdgePolicy; 1] = [in1_policy];
        let out_policies: [EdgePolicy; 0] = [/* empty */];

        let node_id: u32 = 2;
        let in_edge_ids: [u32; 1] = [2];
        let out_edge_ids: [u32; 0] = [/* empty */];

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
            <Self as GraphNodeTypes<2, 1, 0>>::InM,
            <Self as GraphNodeTypes<2, 1, 0>>::OutM,
            C,
            T,
        >::new(
            inputs,
            outputs,
            in_managers,
            out_managers,
            in_policies,
            out_policies,
            node_id,
            in_edge_ids,
            out_edge_ids,
            clock,
            telemetry,
        )
    }

    #[inline]
    fn with_node_and_step_context<'telemetry, 'clock, C, T, R, E>(
        &mut self,
        clock: &'clock C,
        telemetry: &'telemetry mut T,
        f: impl FnOnce(
            &mut <Self as GraphNodeAccess<2>>::Node,
            &mut StepContext<
                '_,
                'telemetry,
                'clock,
                1,
                0,
                <Self as GraphNodeTypes<2, 1, 0>>::InP,
                <Self as GraphNodeTypes<2, 1, 0>>::OutP,
                <Self as GraphNodeTypes<2, 1, 0>>::InQ,
                <Self as GraphNodeTypes<2, 1, 0>>::OutQ,
                <Self as GraphNodeTypes<2, 1, 0>>::InM,
                <Self as GraphNodeTypes<2, 1, 0>>::OutM,
                C,
                T,
            >,
        ) -> Result<R, E>,
    ) -> Result<R, E>
    where
        Self: GraphNodeAccess<2>,
        EdgePolicy: Copy,
        C: PlatformClock + Sized,
        T: Telemetry + Sized,
    {
        let node = &mut self.nodes.2;

        let in1_policy = *self.edges.1.policy();

        let inputs: [&mut <Self as GraphNodeTypes<2, 1, 0>>::InQ; 1] = [self.edges.1.queue_mut()];
        let outputs: [&mut <Self as GraphNodeTypes<2, 1, 0>>::OutQ; 0] = [/* empty */];

        let in_managers: [&mut <Self as GraphNodeTypes<2, 1, 0>>::InM; 1] = [&mut self.managers.1];
        let out_managers: [&mut <Self as GraphNodeTypes<2, 1, 0>>::OutM; 0] = [/* empty */];

        let in_policies: [EdgePolicy; 1] = [in1_policy];
        let out_policies: [EdgePolicy; 0] = [/* empty */];

        let node_id: u32 = 2;
        let in_edge_ids: [u32; 1] = [2];
        let out_edge_ids: [u32; 0] = [/* empty */];

        let mut ctx = StepContext::new(
            inputs,
            outputs,
            in_managers,
            out_managers,
            in_policies,
            out_policies,
            node_id,
            in_edge_ids,
            out_edge_ids,
            clock,
            telemetry,
        );
        f(node, &mut ctx)
    }
}

/// std graph implementation using `ConcurrentEdge` — thread-safe, Clone+Send+Sync edges.
///
/// Topology mirrors `TestPipeline` (source → map → sink), but all real edges are
/// `ConcurrentEdge` so `run_scoped` can hand clones to worker threads without wrapping.
#[cfg(feature = "std")]
pub mod concurrent_graph {
    use super::*;

    use crate::{
        edge::{spsc_concurrent::ConcurrentEdge, EdgeOccupancy, NoQueue},
        errors::{GraphError, NodeError},
        graph::{
            GraphApi, GraphEdgeAccess, GraphNodeAccess, GraphNodeContextBuilder, GraphNodeTypes,
            ScopedGraphApi,
        },
        node::{
            bench::{TestCounterSourceU32_2, TestIdentityModelNodeU32_2},
            source::{
                probe::{new_probe_edge_pair, ConcurrentIngressEdgeLink, SourceIngressUpdater},
                SourceNode,
            },
            StepContext, StepResult,
        },
        policy::{EdgePolicy, WatermarkState},
        prelude::{
            ConcurrentMemoryManager, EdgeDescriptor, EdgeLink, NodeDescriptor, NodeLink,
            PlatformClock, StaticMemoryManager, Telemetry, WorkerDecision, WorkerScheduler,
            WorkerState,
        },
        types::{EdgeIndex, NodeIndex, PortId, PortIndex},
    };

    type ConcMgr32 = ConcurrentMemoryManager<u32>;

    #[allow(type_alias_bounds)]
    type SrcNode<SrcClk: PlatformClock + Send + 'static> =
        SourceNode<TestCounterSourceU32_2<SrcClk, 32>, u32, 1>;

    const TEST_MAX_BATCH: usize = 32;
    type MapNode = TestIdentityModelNodeU32_2<TEST_MAX_BATCH>;
    type SnkNode = SinkNode<TestSinkNodeU32_2, u32, 1>;

    /// Concrete std graph using `ConcurrentEdge` (Arc-backed, Clone+Send+Sync).
    #[allow(clippy::complexity)]
    pub struct TestPipelineStd<SrcClk: PlatformClock + Send + 'static> {
        nodes: (
            NodeLink<SrcNode<SrcClk>, 0, 1, (), u32>,
            NodeLink<MapNode, 1, 1, u32, u32>,
            NodeLink<SnkNode, 1, 0, u32, ()>,
        ),
        edges: (EdgeLink<ConcurrentEdge>, EdgeLink<ConcurrentEdge>),
        ingress_edges: [ConcurrentIngressEdgeLink<u32>; 1],
        ingress_updaters: [Option<SourceIngressUpdater>; 1],
        managers: (ConcMgr32, ConcMgr32),
    }

    impl<SrcClk: PlatformClock + Send + 'static> TestPipelineStd<SrcClk> {
        /// Create a new TestPipelineStd from given nodes, edges and memory managers.
        #[inline]
        pub fn new(
            node_0: impl Into<SrcNode<SrcClk>>,
            node_1: MapNode,
            node_2: impl Into<SnkNode>,
            q_0: ConcurrentEdge,
            q_1: ConcurrentEdge,
            mgr_0: ConcMgr32,
            mgr_1: ConcMgr32,
        ) -> Self {
            let node_0: SrcNode<SrcClk> = node_0.into();
            let node_2: SnkNode = node_2.into();

            let n0 = NodeLink::<SrcNode<SrcClk>, 0, 1, (), u32>::new(
                node_0,
                NodeIndex::from(0usize),
                Some("src"),
            );
            let n1 = NodeLink::<MapNode, 1, 1, u32, u32>::new(
                node_1,
                NodeIndex::from(1usize),
                Some("map"),
            );
            let n2 = NodeLink::<SnkNode, 1, 0, u32, ()>::new(
                node_2,
                NodeIndex::from(2usize),
                Some("snk"),
            );

            let e0 = EdgeLink::new(
                q_0,
                EdgeIndex::from(1usize),
                PortId::new(NodeIndex::from(0usize), PortIndex::from(0)),
                PortId::new(NodeIndex::from(1usize), PortIndex::from(0)),
                Q_32_POLICY,
                Some("e0"),
            );
            let e1 = EdgeLink::new(
                q_1,
                EdgeIndex::from(2usize),
                PortId::new(NodeIndex::from(1usize), PortIndex::from(0)),
                PortId::new(NodeIndex::from(2usize), PortIndex::from(0)),
                Q_32_POLICY,
                Some("e1"),
            );

            let (probe_edge_0, updater_0) = new_probe_edge_pair::<u32>();
            let ingress_edge_0 = ConcurrentIngressEdgeLink::from_probe(
                probe_edge_0,
                EdgeIndex::from(0usize),
                PortId::new(EXTERNAL_INGRESS_NODE, PortIndex::from(0)),
                PortId::new(NodeIndex::from(0usize), PortIndex::from(0)),
                n0.node().source_ref().ingress_policy(),
                Some("ingress0"),
            );

            Self {
                nodes: (n0, n1, n2),
                edges: (e0, e1),
                ingress_edges: [ingress_edge_0],
                ingress_updaters: [Some(updater_0)],
                managers: (mgr_0, mgr_1),
            }
        }

        /// Take the ingress updater for source 0, moving it to the worker thread.
        /// Returns `None` if already taken.
        #[inline]
        pub fn take_ingress_updater_0(&mut self) -> Option<SourceIngressUpdater> {
            self.ingress_updaters[0].take()
        }

        /// Run the graph concurrently using `std::thread::scope` with
        /// scheduler-controlled workers.
        ///
        /// Spawns one thread per node. Each worker queries edge occupancy,
        /// builds a [`WorkerState`] snapshot, and calls `scheduler.decide()`
        /// to get stepping instructions. All threads join when scope exits.
        fn run_scoped_impl<C, T, S>(&mut self, clock: C, telemetry: T, scheduler: S)
        where
            C: PlatformClock + Clone + Send + Sync + 'static,
            T: Telemetry + Clone + Send + 'static,
            S: WorkerScheduler + 'static,
            SrcNode<SrcClk>: Send,
            MapNode: Send,
            SnkNode: Send,
        {
            let pol0 = *self.edges.0.policy();
            let pol1 = *self.edges.1.policy();

            // Clone edges (ConcurrentEdge: Clone via Arc).
            let e0_out = self.edges.0.queue().clone();
            let e0_in = self.edges.0.queue().clone();
            let e1_out = self.edges.1.queue().clone();
            let e1_in = self.edges.1.queue().clone();

            // Clone managers (ConcurrentMemoryManager: Clone).
            let mgr0_n0 = self.managers.0.clone();
            let mgr0_n1 = self.managers.0.clone();
            let mgr1_n1 = self.managers.1.clone();
            let mgr1_n2 = self.managers.1.clone();

            // Clone telemetry.
            let telem0 = telemetry.clone();
            let telem1 = telemetry.clone();
            let telem2 = telemetry;

            // Ingress updater for node 0 (probe reports source depth to scheduler).
            let updater_0 = self.ingress_updaters[0]
                .as_ref()
                .expect("ingress updater missing")
                .clone();

            // Disjoint node borrows — Rust tracks tuple fields as separate locations.
            let n0 = &mut self.nodes.0;
            let n1 = &mut self.nodes.1;
            let n2 = &mut self.nodes.2;

            let clock_ref = &clock;
            let sched_ref = &scheduler;

            std::thread::scope(|scope| {
                // --- Node 0: source (readiness from ingress_occupancy) ---
                {
                    let mut e0_out = e0_out;
                    let mut mgr = mgr0_n0;
                    let mut telem = telem0;
                    scope.spawn(move || {
                        let mut state = WorkerState::new(0, 3, clock_ref.now_ticks());
                        loop {
                            state.current_tick = clock_ref.now_ticks();

                            // Source node: readiness from ingress occupancy.
                            let ingress_occ = n0.node().source_ref().ingress_occupancy();
                            let any_input = *ingress_occ.items() > 0;

                            // Output backpressure from edge 0.
                            let out_occ = e0_out.occupancy(&pol0);
                            state.backpressure = *out_occ.watermark();

                            state.readiness = if !any_input {
                                crate::scheduling::Readiness::NotReady
                            } else if state.backpressure >= WatermarkState::BetweenSoftAndHard {
                                crate::scheduling::Readiness::ReadyUnderPressure
                            } else {
                                crate::scheduling::Readiness::Ready
                            };

                            match sched_ref.decide(&state) {
                                WorkerDecision::Step => {
                                    let mut ctx = StepContext::new(
                                        [] as [&mut NoQueue; 0],
                                        [&mut e0_out],
                                        [] as [&mut StaticMemoryManager<(), 1>; 0],
                                        [&mut mgr],
                                        [] as [EdgePolicy; 0],
                                        [pol0],
                                        0u32,
                                        [] as [u32; 0],
                                        [1u32],
                                        clock_ref,
                                        &mut telem,
                                    );
                                    match n0.step(&mut ctx) {
                                        Ok(sr) => {
                                            state.last_step = Some(sr);
                                            state.last_error = false;
                                        }
                                        Err(_) => {
                                            state.last_step = None;
                                            state.last_error = true;
                                        }
                                    }
                                    // Update ingress probe for cross-thread visibility.
                                    let occ = n0.node().source_ref().ingress_occupancy();
                                    updater_0.update(*occ.items(), *occ.bytes());
                                }
                                WorkerDecision::WaitMicros(d) => {
                                    std::thread::sleep(std::time::Duration::from_micros(d));
                                    state.last_step = None;
                                    state.last_error = false;
                                }
                                WorkerDecision::Stop => break,
                            }
                        }
                    });
                }

                // --- Node 1: map (readiness from input edge 0) ---
                {
                    let mut e0_in = e0_in;
                    let mut e1_out = e1_out;
                    let mut mgr_in = mgr0_n1;
                    let mut mgr_out = mgr1_n1;
                    let mut telem = telem1;
                    scope.spawn(move || {
                        let mut state = WorkerState::new(1, 3, clock_ref.now_ticks());
                        loop {
                            state.current_tick = clock_ref.now_ticks();

                            // Input readiness from edge 0.
                            let in_occ = e0_in.occupancy(&pol0);
                            let any_input = *in_occ.items() > 0;

                            // Output backpressure from edge 1.
                            let out_occ = e1_out.occupancy(&pol1);
                            state.backpressure = *out_occ.watermark();

                            state.readiness = if !any_input {
                                crate::scheduling::Readiness::NotReady
                            } else if state.backpressure >= WatermarkState::BetweenSoftAndHard {
                                crate::scheduling::Readiness::ReadyUnderPressure
                            } else {
                                crate::scheduling::Readiness::Ready
                            };

                            match sched_ref.decide(&state) {
                                WorkerDecision::Step => {
                                    let mut ctx = StepContext::new(
                                        [&mut e0_in],
                                        [&mut e1_out],
                                        [&mut mgr_in],
                                        [&mut mgr_out],
                                        [pol0],
                                        [pol1],
                                        1u32,
                                        [1u32],
                                        [2u32],
                                        clock_ref,
                                        &mut telem,
                                    );
                                    match n1.step(&mut ctx) {
                                        Ok(sr) => {
                                            state.last_step = Some(sr);
                                            state.last_error = false;
                                        }
                                        Err(_) => {
                                            state.last_step = None;
                                            state.last_error = true;
                                        }
                                    }
                                }
                                WorkerDecision::WaitMicros(d) => {
                                    std::thread::sleep(std::time::Duration::from_micros(d));
                                    state.last_step = None;
                                    state.last_error = false;
                                }
                                WorkerDecision::Stop => break,
                            }
                        }
                    });
                }

                // --- Node 2: sink (readiness from input edge 1, no outputs) ---
                {
                    let mut e1_in = e1_in;
                    let mut mgr = mgr1_n2;
                    let mut telem = telem2;
                    scope.spawn(move || {
                        let mut state = WorkerState::new(2, 3, clock_ref.now_ticks());
                        loop {
                            state.current_tick = clock_ref.now_ticks();

                            // Input readiness from edge 1.
                            let in_occ = e1_in.occupancy(&pol1);
                            let any_input = *in_occ.items() > 0;

                            // Sink has no outputs — no backpressure.

                            state.readiness = if !any_input {
                                crate::scheduling::Readiness::NotReady
                            } else {
                                crate::scheduling::Readiness::Ready
                            };

                            match sched_ref.decide(&state) {
                                WorkerDecision::Step => {
                                    let mut ctx = StepContext::new(
                                        [&mut e1_in],
                                        [] as [&mut NoQueue; 0],
                                        [&mut mgr],
                                        [] as [&mut StaticMemoryManager<(), 1>; 0],
                                        [pol1],
                                        [] as [EdgePolicy; 0],
                                        2u32,
                                        [2u32],
                                        [] as [u32; 0],
                                        clock_ref,
                                        &mut telem,
                                    );
                                    match n2.step(&mut ctx) {
                                        Ok(sr) => {
                                            state.last_step = Some(sr);
                                            state.last_error = false;
                                        }
                                        Err(_) => {
                                            state.last_step = None;
                                            state.last_error = true;
                                        }
                                    }
                                }
                                WorkerDecision::WaitMicros(d) => {
                                    std::thread::sleep(std::time::Duration::from_micros(d));
                                    state.last_step = None;
                                    state.last_error = false;
                                }
                                WorkerDecision::Stop => break,
                            }
                        }
                    });
                }
            });
        }
    }

    // ===== ScopedGraphApi<3, 3> =====
    impl<SrcClk: PlatformClock + Clone + Send + Sync + 'static> ScopedGraphApi<3, 3>
        for TestPipelineStd<SrcClk>
    where
        SrcNode<SrcClk>: Send,
    {
        fn run_scoped<C, T, S>(&mut self, clock: C, telemetry: T, scheduler: S)
        where
            C: PlatformClock + Clone + Send + Sync + 'static,
            T: Telemetry + Clone + Send + 'static,
            S: WorkerScheduler + 'static,
        {
            self.run_scoped_impl(clock, telemetry, scheduler)
        }
    }

    // ===== GraphApi<3, 3> =====
    impl<SrcClk: PlatformClock + Send + 'static> GraphApi<3, 3> for TestPipelineStd<SrcClk> {
        #[inline]
        fn get_node_descriptors(&self) -> [NodeDescriptor; 3] {
            [
                self.nodes.0.descriptor(),
                self.nodes.1.descriptor(),
                self.nodes.2.descriptor(),
            ]
        }

        #[inline]
        fn get_edge_descriptors(&self) -> [EdgeDescriptor; 3] {
            [
                self.ingress_edges[0].descriptor(),
                self.edges.0.descriptor(),
                self.edges.1.descriptor(),
            ]
        }

        #[inline]
        fn get_node_policies(&self) -> [NodePolicy; 3] {
            [
                self.nodes.0.policy(),
                self.nodes.1.policy(),
                self.nodes.2.policy(),
            ]
        }

        #[inline]
        fn get_edge_policies(&self) -> [EdgePolicy; 3] {
            [
                self.nodes.0.node().source_ref().ingress_policy(),
                *self.edges.0.policy(),
                *self.edges.1.policy(),
            ]
        }

        #[inline]
        fn edge_occupancy_for<const E: usize>(&self) -> Result<EdgeOccupancy, GraphError> {
            let occ = match E {
                0 => self.ingress_edges[0].occupancy(&self.ingress_edges[0].policy()),
                1 => {
                    let e = &self.edges.0;
                    e.occupancy(e.policy())
                }
                2 => {
                    let e = &self.edges.1;
                    e.occupancy(e.policy())
                }
                _ => return Err(GraphError::InvalidEdgeIndex),
            };
            Ok(occ)
        }

        #[inline]
        fn write_all_edge_occupancies(
            &self,
            out: &mut [EdgeOccupancy; 3],
        ) -> Result<(), GraphError> {
            out[0] = self.edge_occupancy_for::<0>()?;
            out[1] = self.edge_occupancy_for::<1>()?;
            out[2] = self.edge_occupancy_for::<2>()?;
            Ok(())
        }

        #[inline]
        fn refresh_occupancies_for_node<const I: usize, const IN: usize, const OUT: usize>(
            &self,
            out: &mut [EdgeOccupancy; 3],
        ) -> Result<(), GraphError> {
            let node_idx = NodeIndex::from(I);
            for ed in self.get_edge_descriptors().iter() {
                if ed.upstream().node() == &node_idx || ed.downstream().node() == &node_idx {
                    match ed.id().as_usize() {
                        0 => out[0] = self.edge_occupancy_for::<0>()?,
                        1 => out[1] = self.edge_occupancy_for::<1>()?,
                        2 => out[2] = self.edge_occupancy_for::<2>()?,
                        _ => return Err(GraphError::InvalidEdgeIndex),
                    }
                }
            }
            Ok(())
        }

        #[inline]
        fn step_node_by_index<C, T>(
            &mut self,
            index: usize,
            clock: &C,
            telemetry: &mut T,
        ) -> Result<StepResult, NodeError>
        where
            EdgePolicy: Copy,
            C: PlatformClock + Sized,
            T: Telemetry + Sized,
        {
            match index {
                0 => <Self as GraphNodeContextBuilder<0, 0, 1>>::with_node_and_step_context::<
                    C,
                    T,
                    StepResult,
                    NodeError,
                >(self, clock, telemetry, |node, ctx| node.step(ctx)),
                1 => <Self as GraphNodeContextBuilder<1, 1, 1>>::with_node_and_step_context::<
                    C,
                    T,
                    StepResult,
                    NodeError,
                >(self, clock, telemetry, |node, ctx| node.step(ctx)),
                2 => <Self as GraphNodeContextBuilder<2, 1, 0>>::with_node_and_step_context::<
                    C,
                    T,
                    StepResult,
                    NodeError,
                >(self, clock, telemetry, |node, ctx| node.step(ctx)),
                _ => unreachable!("invalid node index"),
            }
        }
    }

    // ===== GraphNodeAccess<I> =====
    impl<SrcClk: PlatformClock + Send + 'static> GraphNodeAccess<0> for TestPipelineStd<SrcClk> {
        type Node = NodeLink<SrcNode<SrcClk>, 0, 1, (), u32>;
        #[inline]
        fn node_ref(&self) -> &Self::Node {
            &self.nodes.0
        }
        #[inline]
        fn node_mut(&mut self) -> &mut Self::Node {
            &mut self.nodes.0
        }
    }
    impl<SrcClk: PlatformClock + Send + 'static> GraphNodeAccess<1> for TestPipelineStd<SrcClk> {
        type Node = NodeLink<MapNode, 1, 1, u32, u32>;
        #[inline]
        fn node_ref(&self) -> &Self::Node {
            &self.nodes.1
        }
        #[inline]
        fn node_mut(&mut self) -> &mut Self::Node {
            &mut self.nodes.1
        }
    }
    impl<SrcClk: PlatformClock + Send + 'static> GraphNodeAccess<2> for TestPipelineStd<SrcClk> {
        type Node = NodeLink<SnkNode, 1, 0, u32, ()>;
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
    impl<SrcClk: PlatformClock + Send + 'static> GraphEdgeAccess<1> for TestPipelineStd<SrcClk> {
        type Edge = EdgeLink<ConcurrentEdge>;
        #[inline]
        fn edge_ref(&self) -> &Self::Edge {
            &self.edges.0
        }
        #[inline]
        fn edge_mut(&mut self) -> &mut Self::Edge {
            &mut self.edges.0
        }
    }
    impl<SrcClk: PlatformClock + Send + 'static> GraphEdgeAccess<2> for TestPipelineStd<SrcClk> {
        type Edge = EdgeLink<ConcurrentEdge>;
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
    impl<SrcClk: PlatformClock + Send + 'static> GraphNodeTypes<0, 0, 1> for TestPipelineStd<SrcClk> {
        type InP = ();
        type OutP = u32;
        type InQ = NoQueue;
        type OutQ = ConcurrentEdge;
        type InM = StaticMemoryManager<(), 1>;
        type OutM = ConcMgr32;
    }
    impl<SrcClk: PlatformClock + Send + 'static> GraphNodeTypes<1, 1, 1> for TestPipelineStd<SrcClk> {
        type InP = u32;
        type OutP = u32;
        type InQ = ConcurrentEdge;
        type OutQ = ConcurrentEdge;
        type InM = ConcMgr32;
        type OutM = ConcMgr32;
    }
    impl<SrcClk: PlatformClock + Send + 'static> GraphNodeTypes<2, 1, 0> for TestPipelineStd<SrcClk> {
        type InP = u32;
        type OutP = ();
        type InQ = ConcurrentEdge;
        type OutQ = NoQueue;
        type InM = ConcMgr32;
        type OutM = StaticMemoryManager<(), 1>;
    }

    // ===== GraphNodeContextBuilder<I, IN, OUT> =====

    // node 0: in=[], out=[e0]
    impl<SrcClk: PlatformClock + Send + 'static> GraphNodeContextBuilder<0, 0, 1>
        for TestPipelineStd<SrcClk>
    where
        Self: GraphNodeAccess<0, Node = NodeLink<SrcNode<SrcClk>, 0, 1, (), u32>>,
    {
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
            <Self as GraphNodeTypes<0, 0, 1>>::InM,
            <Self as GraphNodeTypes<0, 0, 1>>::OutM,
            C,
            T,
        >
        where
            EdgePolicy: Copy,
            C: PlatformClock + Sized,
            T: Telemetry + Sized,
        {
            let out0_policy = *self.edges.0.policy();
            StepContext::new(
                [],
                [self.edges.0.queue_mut()],
                [],
                [&mut self.managers.0],
                [],
                [out0_policy],
                0u32,
                [],
                [1u32],
                clock,
                telemetry,
            )
        }

        #[inline]
        fn with_node_and_step_context<'telemetry, 'clock, C, T, R, E>(
            &mut self,
            clock: &'clock C,
            telemetry: &'telemetry mut T,
            f: impl FnOnce(
                &mut <Self as GraphNodeAccess<0>>::Node,
                &mut StepContext<
                    '_,
                    'telemetry,
                    'clock,
                    0,
                    1,
                    <Self as GraphNodeTypes<0, 0, 1>>::InP,
                    <Self as GraphNodeTypes<0, 0, 1>>::OutP,
                    <Self as GraphNodeTypes<0, 0, 1>>::InQ,
                    <Self as GraphNodeTypes<0, 0, 1>>::OutQ,
                    <Self as GraphNodeTypes<0, 0, 1>>::InM,
                    <Self as GraphNodeTypes<0, 0, 1>>::OutM,
                    C,
                    T,
                >,
            ) -> Result<R, E>,
        ) -> Result<R, E>
        where
            Self: GraphNodeAccess<0>,
            EdgePolicy: Copy,
            C: PlatformClock + Sized,
            T: Telemetry + Sized,
        {
            let node = &mut self.nodes.0;
            let out0_policy = *self.edges.0.policy();
            let outputs = [self.edges.0.queue_mut()];
            let out_managers = [&mut self.managers.0];
            let mut ctx = StepContext::new(
                [],
                outputs,
                [],
                out_managers,
                [],
                [out0_policy],
                0u32,
                [],
                [1u32],
                clock,
                telemetry,
            );
            f(node, &mut ctx)
        }
    }

    // node 1: in=[e0], out=[e1]
    impl<SrcClk: PlatformClock + Send + 'static> GraphNodeContextBuilder<1, 1, 1>
        for TestPipelineStd<SrcClk>
    where
        Self: GraphNodeAccess<1, Node = NodeLink<MapNode, 1, 1, u32, u32>>,
    {
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
            <Self as GraphNodeTypes<1, 1, 1>>::InM,
            <Self as GraphNodeTypes<1, 1, 1>>::OutM,
            C,
            T,
        >
        where
            EdgePolicy: Copy,
            C: PlatformClock + Sized,
            T: Telemetry + Sized,
        {
            let in0_policy = *self.edges.0.policy();
            let out1_policy = *self.edges.1.policy();
            StepContext::new(
                [self.edges.0.queue_mut()],
                [self.edges.1.queue_mut()],
                [&mut self.managers.0],
                [&mut self.managers.1],
                [in0_policy],
                [out1_policy],
                1u32,
                [1u32],
                [2u32],
                clock,
                telemetry,
            )
        }

        #[inline]
        fn with_node_and_step_context<'telemetry, 'clock, C, T, R, E>(
            &mut self,
            clock: &'clock C,
            telemetry: &'telemetry mut T,
            f: impl FnOnce(
                &mut <Self as GraphNodeAccess<1>>::Node,
                &mut StepContext<
                    '_,
                    'telemetry,
                    'clock,
                    1,
                    1,
                    <Self as GraphNodeTypes<1, 1, 1>>::InP,
                    <Self as GraphNodeTypes<1, 1, 1>>::OutP,
                    <Self as GraphNodeTypes<1, 1, 1>>::InQ,
                    <Self as GraphNodeTypes<1, 1, 1>>::OutQ,
                    <Self as GraphNodeTypes<1, 1, 1>>::InM,
                    <Self as GraphNodeTypes<1, 1, 1>>::OutM,
                    C,
                    T,
                >,
            ) -> Result<R, E>,
        ) -> Result<R, E>
        where
            Self: GraphNodeAccess<1>,
            EdgePolicy: Copy,
            C: PlatformClock + Sized,
            T: Telemetry + Sized,
        {
            let node = &mut self.nodes.1;
            let in0_policy = *self.edges.0.policy();
            let out1_policy = *self.edges.1.policy();
            let inputs = [self.edges.0.queue_mut()];
            let outputs = [self.edges.1.queue_mut()];
            let in_mgrs = [&mut self.managers.0];
            let out_mgrs = [&mut self.managers.1];
            let mut ctx = StepContext::new(
                inputs,
                outputs,
                in_mgrs,
                out_mgrs,
                [in0_policy],
                [out1_policy],
                1u32,
                [1u32],
                [2u32],
                clock,
                telemetry,
            );
            f(node, &mut ctx)
        }
    }

    // node 2: in=[e1], out=[]
    impl<SrcClk: PlatformClock + Send + 'static> GraphNodeContextBuilder<2, 1, 0>
        for TestPipelineStd<SrcClk>
    where
        Self: GraphNodeAccess<2, Node = NodeLink<SnkNode, 1, 0, u32, ()>>,
    {
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
            <Self as GraphNodeTypes<2, 1, 0>>::InM,
            <Self as GraphNodeTypes<2, 1, 0>>::OutM,
            C,
            T,
        >
        where
            EdgePolicy: Copy,
            C: PlatformClock + Sized,
            T: Telemetry + Sized,
        {
            let in1_policy = *self.edges.1.policy();
            StepContext::new(
                [self.edges.1.queue_mut()],
                [],
                [&mut self.managers.1],
                [],
                [in1_policy],
                [],
                2u32,
                [2u32],
                [],
                clock,
                telemetry,
            )
        }

        #[inline]
        fn with_node_and_step_context<'telemetry, 'clock, C, T, R, E>(
            &mut self,
            clock: &'clock C,
            telemetry: &'telemetry mut T,
            f: impl FnOnce(
                &mut <Self as GraphNodeAccess<2>>::Node,
                &mut StepContext<
                    '_,
                    'telemetry,
                    'clock,
                    1,
                    0,
                    <Self as GraphNodeTypes<2, 1, 0>>::InP,
                    <Self as GraphNodeTypes<2, 1, 0>>::OutP,
                    <Self as GraphNodeTypes<2, 1, 0>>::InQ,
                    <Self as GraphNodeTypes<2, 1, 0>>::OutQ,
                    <Self as GraphNodeTypes<2, 1, 0>>::InM,
                    <Self as GraphNodeTypes<2, 1, 0>>::OutM,
                    C,
                    T,
                >,
            ) -> Result<R, E>,
        ) -> Result<R, E>
        where
            Self: GraphNodeAccess<2>,
            EdgePolicy: Copy,
            C: PlatformClock + Sized,
            T: Telemetry + Sized,
        {
            let node = &mut self.nodes.2;
            let in1_policy = *self.edges.1.policy();
            let inputs = [self.edges.1.queue_mut()];
            let in_mgrs = [&mut self.managers.1];
            let mut ctx = StepContext::new(
                inputs,
                [],
                in_mgrs,
                [],
                [in1_policy],
                [],
                2u32,
                [2u32],
                [],
                clock,
                telemetry,
            );
            f(node, &mut ctx)
        }
    }
}
