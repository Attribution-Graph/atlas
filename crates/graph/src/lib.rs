//! # graph
//!
//! Contribution graph construction and PageRank-style ranking for Atlas.
//!
//! ## Overview
//!
//! This crate provides:
//! - [`builder::ContributionGraph`]: directed weighted graph of account interactions
//! - [`builder::GraphBuilder`]: builds a graph from transaction records
//! - [`rank::compute_pagerank`]: iterative PageRank-style ranking over the graph
//!
//! ## Example
//!
//! ```rust
//! use graph::builder::{GraphBuilder, TxRecord};
//! use graph::rank::{compute_pagerank, RankConfig};
//!
//! let mut tx = TxRecord::new("ALICE", 100);
//! tx.add_destination("BOB");
//!
//! let graph = GraphBuilder::new().build(&[tx]).unwrap();
//! let result = compute_pagerank(&graph, &RankConfig::default()).unwrap();
//!
//! for entry in result.top(5) {
//!     println!("{}: {:.4}", entry.account_id, entry.score);
//! }
//! ```

pub mod builder;
pub mod rank;

pub use builder::{ContributionGraph, Edge, GraphBuilder, Node, TxRecord};
pub use rank::{compute_pagerank, RankConfig, RankEntry, RankingResult};

use thiserror::Error;

/// Errors that can occur during graph construction and ranking.
#[derive(Debug, Error)]
pub enum GraphError {
    #[error("Cannot build graph from empty input")]
    EmptyInput,

    #[error("Cannot rank an empty graph")]
    EmptyGraph,

    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}
