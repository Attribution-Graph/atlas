//! Contribution graph builder.
//!
//! Constructs a directed weighted graph from a stream of Stellar transactions.
//! Each unique account is a node; each transaction creates directed edges from
//! the source account to destination accounts, weighted by operation count and
//! transaction fees.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::GraphError;

/// A node in the contribution graph, representing a Stellar account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Node {
    /// Unique internal ID for this node.
    pub id: Uuid,
    /// Stellar account ID (public key).
    pub account_id: String,
    /// Optional human-readable label.
    pub label: Option<String>,
    /// Total number of transactions this account participated in.
    pub transaction_count: u64,
    /// Total fees paid by this account (in stroops).
    pub total_fees: u64,
}

impl Node {
    /// Create a new node for the given account.
    pub fn new(account_id: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            account_id: account_id.into(),
            label: None,
            transaction_count: 0,
            total_fees: 0,
        }
    }
}

/// A directed weighted edge between two nodes in the contribution graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// The source node's account ID.
    pub from: String,
    /// The destination node's account ID.
    pub to: String,
    /// Edge weight: cumulative interaction strength.
    pub weight: f64,
    /// Number of transactions that contributed to this edge.
    pub transaction_count: u64,
}

impl Edge {
    fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            weight: 0.0,
            transaction_count: 0,
        }
    }

    fn add_interaction(&mut self, weight_delta: f64) {
        self.weight += weight_delta;
        self.transaction_count += 1;
    }
}

/// A lightweight transaction record consumed by the graph builder.
#[derive(Debug, Clone)]
pub struct TxRecord {
    /// Source account of the transaction.
    pub source: String,
    /// Destination accounts involved in operations.
    pub destinations: Vec<String>,
    /// Fee charged for the transaction (in stroops).
    pub fee: u64,
    /// Number of operations in the transaction.
    pub op_count: usize,
    /// Whether the transaction succeeded.
    pub successful: bool,
}

impl TxRecord {
    /// Create a new transaction record.
    pub fn new(source: impl Into<String>, fee: u64) -> Self {
        Self {
            source: source.into(),
            destinations: Vec::new(),
            fee,
            op_count: 0,
            successful: true,
        }
    }

    /// Add a destination account to this record.
    pub fn add_destination(&mut self, dest: impl Into<String>) {
        self.destinations.push(dest.into());
        self.op_count += 1;
    }
}

/// The contribution graph: a directed weighted graph of account interactions.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ContributionGraph {
    /// Nodes keyed by account ID.
    pub nodes: HashMap<String, Node>,
    /// Edges keyed by `"{from}->{to}"`.
    pub edges: HashMap<String, Edge>,
}

impl ContributionGraph {
    /// Create an empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Get a node by account ID.
    pub fn get_node(&self, account_id: &str) -> Option<&Node> {
        self.nodes.get(account_id)
    }

    /// Get an edge by source and destination account IDs.
    pub fn get_edge(&self, from: &str, to: &str) -> Option<&Edge> {
        let key = edge_key(from, to);
        self.edges.get(&key)
    }

    /// Return all outgoing edges from the given node.
    pub fn outgoing_edges(&self, account_id: &str) -> Vec<&Edge> {
        self.edges
            .values()
            .filter(|e| e.from == account_id)
            .collect()
    }

    /// Return all incoming edges to the given node.
    pub fn incoming_edges(&self, account_id: &str) -> Vec<&Edge> {
        self.edges.values().filter(|e| e.to == account_id).collect()
    }

    /// Add or retrieve a node, returning a mutable reference.
    fn ensure_node(&mut self, account_id: &str) -> &mut Node {
        self.nodes
            .entry(account_id.to_string())
            .or_insert_with(|| Node::new(account_id))
    }

    /// Add or update an edge between two nodes.
    fn ensure_edge(&mut self, from: &str, to: &str) -> &mut Edge {
        let key = edge_key(from, to);
        self.edges.entry(key).or_insert_with(|| Edge::new(from, to))
    }
}

fn edge_key(from: &str, to: &str) -> String {
    format!("{}=>{}", from, to)
}

/// Builds a [`ContributionGraph`] from a stream of transaction records.
#[derive(Debug, Default)]
pub struct GraphBuilder {
    /// Weight multiplier applied to successful transactions.
    success_weight: f64,
    /// Base weight for each operation.
    op_weight: f64,
    /// Weight contribution from fees (per stroop).
    fee_weight_factor: f64,
}

impl GraphBuilder {
    /// Create a builder with default weighting parameters.
    pub fn new() -> Self {
        Self {
            success_weight: 1.5,
            op_weight: 1.0,
            fee_weight_factor: 0.0001,
        }
    }

    /// Set the success weight multiplier.
    pub fn success_weight(mut self, w: f64) -> Self {
        self.success_weight = w;
        self
    }

    /// Set the base weight per operation.
    pub fn op_weight(mut self, w: f64) -> Self {
        self.op_weight = w;
        self
    }

    /// Build a [`ContributionGraph`] from the given transaction records.
    ///
    /// # Errors
    /// Returns [`GraphError::EmptyInput`] if the records slice is empty.
    pub fn build(&self, records: &[TxRecord]) -> Result<ContributionGraph, GraphError> {
        if records.is_empty() {
            return Err(GraphError::EmptyInput);
        }

        let mut graph = ContributionGraph::new();

        for tx in records {
            // Only process successful transactions for edge weighting
            let base_weight = if tx.successful {
                self.success_weight
            } else {
                0.5
            };

            let op_contribution = (tx.op_count as f64) * self.op_weight;
            let fee_contribution = (tx.fee as f64) * self.fee_weight_factor;
            let edge_weight = base_weight * (op_contribution + fee_contribution).max(1.0);

            // Update source node
            {
                let src_node = graph.ensure_node(&tx.source);
                src_node.transaction_count += 1;
                src_node.total_fees += tx.fee;
            }

            // Add edges from source to each destination
            for dest in &tx.destinations {
                if dest == &tx.source {
                    continue; // skip self-loops
                }

                // Ensure destination node exists
                graph.ensure_node(dest);

                // Update edge
                let edge = graph.ensure_edge(&tx.source, dest);
                edge.add_interaction(edge_weight);
            }

            // If no destinations, source still appears as isolated node
        }

        Ok(graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tx(source: &str, destinations: &[&str], fee: u64) -> TxRecord {
        let mut tx = TxRecord::new(source, fee);
        for d in destinations {
            tx.add_destination(*d);
        }
        tx
    }

    #[test]
    fn test_build_simple_graph() {
        let records = vec![
            make_tx("ALICE", &["BOB"], 100),
            make_tx("BOB", &["CAROL"], 200),
            make_tx("ALICE", &["CAROL"], 150),
        ];

        let graph = GraphBuilder::new().build(&records).unwrap();
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 3);

        let alice_node = graph.get_node("ALICE").unwrap();
        assert_eq!(alice_node.transaction_count, 2);
        assert_eq!(alice_node.total_fees, 250);

        let edge = graph.get_edge("ALICE", "BOB").unwrap();
        assert!(edge.weight > 0.0);
        assert_eq!(edge.transaction_count, 1);
    }

    #[test]
    fn test_empty_input_error() {
        let result = GraphBuilder::new().build(&[]);
        assert!(matches!(result, Err(GraphError::EmptyInput)));
    }

    #[test]
    fn test_no_self_loops() {
        let records = vec![make_tx("ALICE", &["ALICE", "BOB"], 100)];
        let graph = GraphBuilder::new().build(&records).unwrap();
        assert!(graph.get_edge("ALICE", "ALICE").is_none());
        assert!(graph.get_edge("ALICE", "BOB").is_some());
    }

    #[test]
    fn test_edge_accumulates() {
        let records = vec![
            make_tx("ALICE", &["BOB"], 100),
            make_tx("ALICE", &["BOB"], 100),
        ];
        let graph = GraphBuilder::new().build(&records).unwrap();
        let edge = graph.get_edge("ALICE", "BOB").unwrap();
        assert_eq!(edge.transaction_count, 2);
        assert!(edge.weight > 0.0);
    }

    #[test]
    fn test_outgoing_incoming_edges() {
        let records = vec![make_tx("ALICE", &["BOB", "CAROL"], 100)];
        let graph = GraphBuilder::new().build(&records).unwrap();

        let outgoing = graph.outgoing_edges("ALICE");
        assert_eq!(outgoing.len(), 2);

        let incoming = graph.incoming_edges("BOB");
        assert_eq!(incoming.len(), 1);
    }
}
