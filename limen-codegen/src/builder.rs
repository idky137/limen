// limen-codegen/src/builder.rs
//! LS-friendly typed builder for Limen graphs (no proc-macro, no big strings).
//!
//! Use with `syn::parse_quote!` in `build.rs` to author types and policies as
//! normal Rust expressions, then call `expand_ast_to_file(..)`.

use crate::ast;
use syn::{Expr, Ident, Type, TypePath, Visibility};

#[derive(Default)]
pub struct GraphBuilder {
    vis: Option<Visibility>,
    name: Option<Ident>,
    nodes: Vec<ast::NodeDef>,
    edges: Vec<ast::EdgeDef>,
}

impl GraphBuilder {
    pub fn new(vis: Visibility, name: Ident) -> Self {
        Self {
            vis: Some(vis),
            name: Some(name),
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn node(mut self, n: Node) -> Self {
        self.nodes.push(n.finish());
        self
    }

    pub fn edge(mut self, e: Edge) -> Self {
        self.edges.push(e.finish());
        self
    }

    pub fn finish(self) -> ast::GraphDef {
        ast::GraphDef {
            vis: self.vis.expect("visibility required"),
            name: self.name.expect("name required"),
            nodes: self.nodes,
            edges: self.edges,
        }
    }
}

pub struct Node {
    idx: usize,
    ty: Option<TypePath>,
    in_ports: Option<usize>,
    out_ports: Option<usize>,
    in_payload: Option<Type>,
    out_payload: Option<Type>,
    name_opt: Option<Expr>,
    ingress_policy_opt: Option<Expr>,
}

impl Node {
    pub fn new(idx: usize) -> Self {
        Self {
            idx,
            ty: None,
            in_ports: None,
            out_ports: None,
            in_payload: None,
            out_payload: None,
            name_opt: None,
            ingress_policy_opt: None,
        }
    }
    pub fn ty(mut self, t: TypePath) -> Self {
        self.ty = Some(t);
        self
    }
    pub fn in_ports(mut self, n: usize) -> Self {
        self.in_ports = Some(n);
        self
    }
    pub fn out_ports(mut self, n: usize) -> Self {
        self.out_ports = Some(n);
        self
    }
    pub fn in_payload(mut self, t: Type) -> Self {
        self.in_payload = Some(t);
        self
    }
    pub fn out_payload(mut self, t: Type) -> Self {
        self.out_payload = Some(t);
        self
    }
    pub fn name(mut self, e: Expr) -> Self {
        self.name_opt = Some(e);
        self
    }
    pub fn ingress_policy(mut self, e: Expr) -> Self {
        self.ingress_policy_opt = Some(e);
        self
    }

    fn finish(self) -> ast::NodeDef {
        ast::NodeDef {
            idx: self.idx,
            ty: self.ty.expect("node.ty"),
            in_ports: self.in_ports.expect("node.in_ports"),
            out_ports: self.out_ports.expect("node.out_ports"),
            in_payload: self.in_payload.expect("node.in_payload"),
            out_payload: self.out_payload.expect("node.out_payload"),
            name_opt: self.name_opt,
            ingress_policy_opt: self.ingress_policy_opt,
        }
    }
}

pub struct Edge {
    idx: usize,
    ty: Option<TypePath>,
    payload: Option<Type>,
    from_node: Option<usize>,
    from_port: Option<usize>,
    to_node: Option<usize>,
    to_port: Option<usize>,
    policy: Option<Expr>,
    name_opt: Option<Expr>,
}

impl Edge {
    pub fn new(idx: usize) -> Self {
        Self {
            idx,
            ty: None,
            payload: None,
            from_node: None,
            from_port: None,
            to_node: None,
            to_port: None,
            policy: None,
            name_opt: None,
        }
    }
    pub fn ty(mut self, t: TypePath) -> Self {
        self.ty = Some(t);
        self
    }
    pub fn payload(mut self, t: Type) -> Self {
        self.payload = Some(t);
        self
    }
    pub fn from(mut self, node: usize, port: usize) -> Self {
        self.from_node = Some(node);
        self.from_port = Some(port);
        self
    }
    pub fn to(mut self, node: usize, port: usize) -> Self {
        self.to_node = Some(node);
        self.to_port = Some(port);
        self
    }
    pub fn policy(mut self, e: Expr) -> Self {
        self.policy = Some(e);
        self
    }
    pub fn name(mut self, e: Expr) -> Self {
        self.name_opt = Some(e);
        self
    }

    fn finish(self) -> ast::EdgeDef {
        ast::EdgeDef {
            idx: self.idx,
            ty: self.ty.expect("edge.ty"),
            payload: self.payload.expect("edge.payload"),
            from_node: self.from_node.expect("edge.from.node"),
            from_port: self.from_port.expect("edge.from.port"),
            to_node: self.to_node.expect("edge.to.node"),
            to_port: self.to_port.expect("edge.to.port"),
            policy: self.policy.expect("edge.policy"),
            name_opt: self.name_opt,
        }
    }
}
