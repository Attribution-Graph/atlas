//! PageRank-style contribution ranking algorithm.
//!
//! Implements a weighted PageRank over the contribution graph. Each node's
//! rank is iteratively computed as a function of the ranks of nodes that
//! link to it, weighted by edge strength. The algorithm converges when the
//! maximum rank delta between iterations falls below a configurable tolerance.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::builder::ContributionGraph;
use crate::GraphError;

/// Configuration for the PageRank computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankConfig {
    /// Damping factor (typically 0.85).
    pub damping: f64,
    /// Maximum number of iterations.
    pub max_iterations: u32,
    /// Convergence tolerance (stops when max delta < tolerance).
    pub tolerance: f64,
}

impl Default for RankConfig {
    fn default() -> Self {
        Self {
            damping: 0.85,
            max_iterations: 100,
            tolerance: 1e-6,
        }
    }
}

impl RankConfig {
    /// Create a strict config for high-precision ranking.
    pub fn strict() -> Self {
        Self {
            tolerance: 1e-10,
            max_iterations: 500,
            ..Default::default()
        }
    }
}

/// The final ranking result for a single node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankEntry {
    /// Stellar account ID.
    pub account_id: String,
    /// Normalized PageRank score in [0, 1].
    pub score: f64,
    /// Zero-based rank position (0 = highest ranked).
    pub rank: usize,
    /// Number of incoming edges (in-degree).
    pub in_degree: usize,
    /// Number of outgoing edges (out-degree).
    pub out_degree: usize,
}

/// The output of a ranking computation.
#[derive(Debug, Serialize, Deserialize)]
pub struct RankingResult {
    /// Ranked entries sorted by descending score.
    pub entries: Vec<RankEntry>,
    /// Number of iterations until convergence (or max_iterations).
    pub iterations: u32,
    /// Whether the algorithm converged within tolerance.
    pub converged: bool,
}

impl RankingResult {
    /// Get the top N ranked entries.
    pub fn top(&self, n: usize) -> &[RankEntry] {
        let end = n.min(self.entries.len());
        &self.entries[..end]
    }

    /// Look up the rank entry for a specific account.
    pub fn get(&self, account_id: &str) -> Option<&RankEntry> {
        self.entries.iter().find(|e| e.account_id == account_id)
    }
}

/// Compute PageRank-style contribution scores for all nodes in a graph.
///
/// # Algorithm
///
/// Standard weighted PageRank:
/// ```text
/// PR(v) = (1 - d) / N + d * Σ [ PR(u) * w(u,v) / W(u) ]
/// ```
/// where:
/// - `d` is the damping factor
/// - `N` is the number of nodes
/// - `w(u,v)` is the weight of edge u→v
/// - `W(u)` is the total outgoing weight from node u
///
/// # Errors
/// Returns [`GraphError::EmptyGraph`] if the graph has no nodes.
pub fn compute_pagerank(
    graph: &ContributionGraph,
    config: &RankConfig,
) -> Result<RankingResult, GraphError> {
    let node_ids: Vec<String> = graph.nodes.keys().cloned().collect();
    let n = node_ids.len();

    if n == 0 {
        return Err(GraphError::EmptyGraph);
    }

    // Initialize ranks uniformly
    let initial_rank = 1.0 / n as f64;
    let mut ranks: HashMap<String, f64> =
        node_ids.iter().map(|id| (id.clone(), initial_rank)).collect();

    // Precompute total outgoing weight for each node
    let out_weights: HashMap<String, f64> = node_ids
        .iter()
        .map(|id| {
            let total_weight: f64 = graph.outgoing_edges(id).iter().map(|e| e.weight).sum();
            (id.clone(), total_weight)
        })
        .collect();

    let dangling_factor = (1.0 - config.damping) / n as f64;
    let mut iterations = 0u32;
    let mut converged = false;

    for iter in 0..config.max_iterations {
        iterations = iter + 1;
        let mut new_ranks: HashMap<String, f64> = node_ids
            .iter()
            .map(|id| (id.clone(), dangling_factor))
            .collect();

        // Distribute rank from each node to its neighbors
        for node_id in &node_ids {
            let current_rank = ranks[node_id];
            let total_out = out_weights[node_id];

            if total_out <= 0.0 {
                // Dangling node: distribute rank equally to all nodes
                let dangling_contribution = config.damping * current_rank / n as f64;
                for target in &node_ids {
                    *new_ranks.get_mut(target).unwrap() += dangling_contribution;
                }
                continue;
            }

            for edge in graph.outgoing_edges(node_id) {
                let contribution = config.damping * current_rank * edge.weight / total_out;
                if let Some(r) = new_ranks.get_mut(&edge.to) {
                    *r += contribution;
                }
            }
        }

        // Check convergence
        let max_delta = node_ids
            .iter()
            .map(|id| (new_ranks[id] - ranks[id]).abs())
            .fold(0.0f64, f64::max);

        ranks = new_ranks;

        if max_delta < config.tolerance {
            converged = true;
            break;
        }
    }

    // Normalize scores to [0, 1]
    let max_rank = ranks.values().cloned().fold(0.0f64, f64::max);
    let normalizer = if max_rank > 0.0 { max_rank } else { 1.0 };

    // Build sorted rank entries
    let mut entries: Vec<RankEntry> = node_ids
        .iter()
        .map(|id| {
            let score = ranks[id] / normalizer;
            let in_degree = graph.incoming_edges(id).len();
            let out_degree = graph.outgoing_edges(id).len();
            RankEntry {
                account_id: id.clone(),
                score,
                rank: 0, // filled in after sort
                in_degree,
                out_degree,
            }
        })
        .collect();

    // Sort descending by score, then by account_id for determinism
    entries.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.account_id.cmp(&b.account_id))
    });

    // Assign rank positions
    for (i, entry) in entries.iter_mut().enumerate() {
        entry.rank = i;
    }

    Ok(RankingResult {
        entries,
        iterations,
        converged,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{GraphBuilder, TxRecord};

    fn make_tx(source: &str, destinations: &[&str]) -> TxRecord {
        let mut tx = TxRecord::new(source, 100);
        for d in destinations {
            tx.add_destination(*d);
        }
        tx
    }

    #[test]
    fn test_pagerank_basic() {
        // Classic PageRank example: A→B, A→C, B→C
        // C should have the highest rank
        let records = vec![
            make_tx("A", &["B", "C"]),
            make_tx("B", &["C"]),
        ];

        let graph = GraphBuilder::new().build(&records).unwrap();
        let config = RankConfig::default();
        let result = compute_pagerank(&graph, &config).unwrap();

        assert!(!result.entries.is_empty());
        assert_eq!(result.entries[0].account_id, "C");
        assert!(result.entries[0].score <= 1.0);
        assert!(result.entries[0].score >= 0.0);
    }

    #[test]
    fn test_pagerank_converges() {
        let records = vec![
            make_tx("A", &["B"]),
            make_tx("B", &["C"]),
            make_tx("C", &["A"]),
        ];
        let graph = GraphBuilder::new().build(&records).unwrap();
        let result = compute_pagerank(&graph, &RankConfig::default()).unwrap();
        assert!(result.converged);
    }

    #[test]
    fn test_scores_normalized() {
        let records = vec![
            make_tx("A", &["B", "C"]),
            make_tx("B", &["C"]),
        ];
        let graph = GraphBuilder::new().build(&records).unwrap();
        let result = compute_pagerank(&graph, &RankConfig::default()).unwrap();

        for entry in &result.entries {
            assert!(entry.score >= 0.0 && entry.score <= 1.0);
        }
    }

    #[test]
    fn test_empty_graph_error() {
        let graph = ContributionGraph::new();
        let result = compute_pagerank(&graph, &RankConfig::default());
        assert!(matches!(result, Err(GraphError::EmptyGraph)));
    }

    #[test]
    fn test_top_n() {
        let records = vec![
            make_tx("A", &["B"]),
            make_tx("B", &["C"]),
            make_tx("C", &["D"]),
        ];
        let graph = GraphBuilder::new().build(&records).unwrap();
        let result = compute_pagerank(&graph, &RankConfig::default()).unwrap();

        let top2 = result.top(2);
        assert_eq!(top2.len(), 2);
        assert!(top2[0].score >= top2[1].score);
    }

    #[test]
    fn test_rank_positions_assigned() {
        let records = vec![
            make_tx("A", &["B"]),
            make_tx("B", &["C"]),
        ];
        let graph = GraphBuilder::new().build(&records).unwrap();
        let result = compute_pagerank(&graph, &RankConfig::default()).unwrap();

        for (i, entry) in result.entries.iter().enumerate() {
            assert_eq!(entry.rank, i);
        }
    }
}
