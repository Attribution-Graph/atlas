//! Orchestrates the ingestion and graph-ranking pipeline.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use graph::builder::{GraphBuilder, TxRecord};
use graph::rank::{compute_pagerank, RankConfig, RankingResult};
use ingestion::horizon::{HorizonClient, HorizonConfig};
use ingestion::ledger_range::LedgerRange;
use ingestion::models::Transaction;

/// Configuration for a single ingestion run.
#[derive(Debug, Clone)]
pub struct IngestConfig {
    /// Base URL of the Horizon server.
    pub horizon_url: String,
    /// Starting ledger (inclusive).
    pub start_ledger: u32,
    /// Ending ledger (inclusive).
    pub end_ledger: u32,
    /// PageRank damping factor.
    pub damping: f64,
    /// Maximum PageRank iterations.
    pub max_iterations: u32,
    /// Optional PostgreSQL URL for persistence (handled by caller post-run).
    #[allow(dead_code)]
    pub database_url: Option<String>,
    /// Number of top entries to display (handled by caller post-run).
    #[allow(dead_code)]
    pub top_n: usize,
}

/// The result of a complete ingestion and ranking run.
#[derive(Debug, Serialize, Deserialize)]
pub struct IngestResult {
    pub start_ledger: u32,
    pub end_ledger: u32,
    pub transaction_count: usize,
    pub participant_count: usize,
    pub ranking: RankingResult,
}

/// Run the full ingestion → graph construction → ranking pipeline.
pub async fn run(config: IngestConfig) -> Result<IngestResult> {
    let horizon_config = HorizonConfig::with_url(&config.horizon_url);
    let client = HorizonClient::new(horizon_config)?;

    // Step 1: Validate and build the ledger range
    let range = LedgerRange::new(config.start_ledger, config.end_ledger)
        .map_err(|e| anyhow::anyhow!("Invalid ledger range: {}", e))?;

    info!(
        range = %range,
        size = range.size(),
        "Ingesting ledger range"
    );

    // Warn if range is very large
    if let Err(e) = range.check_size() {
        tracing::warn!(
            "{} — consider using --start-ledger/--end-ledger to narrow the window",
            e
        );
    }

    let transactions = client
        .fetch_transactions_in_range(range.start(), range.end())
        .await?;

    info!(count = transactions.len(), "Transactions ingested");

    if transactions.is_empty() {
        anyhow::bail!(
            "No transactions found in ledger range {}-{}",
            config.start_ledger,
            config.end_ledger
        );
    }

    // Step 2: Convert to TxRecords for the graph builder
    let records = build_tx_records(&transactions);
    let participant_count = count_unique_participants(&records);
    debug!(
        participants = participant_count,
        "Unique participants found"
    );

    // Step 3: Build contribution graph
    let graph = GraphBuilder::new()
        .build(&records)
        .map_err(|e| anyhow::anyhow!("Graph construction failed: {}", e))?;

    info!(
        nodes = graph.node_count(),
        edges = graph.edge_count(),
        "Contribution graph built"
    );

    // Step 4: Compute PageRank
    let rank_config = RankConfig {
        damping: config.damping,
        max_iterations: config.max_iterations,
        ..Default::default()
    };

    let ranking = compute_pagerank(&graph, &rank_config)
        .map_err(|e| anyhow::anyhow!("Ranking failed: {}", e))?;

    info!(
        converged = ranking.converged,
        iterations = ranking.iterations,
        "PageRank computation complete"
    );

    Ok(IngestResult {
        start_ledger: config.start_ledger,
        end_ledger: config.end_ledger,
        transaction_count: transactions.len(),
        participant_count,
        ranking,
    })
}

/// Convert Stellar transactions into lightweight TxRecord graph inputs.
fn build_tx_records(transactions: &[Transaction]) -> Vec<TxRecord> {
    transactions
        .iter()
        .map(|tx| {
            let mut record = TxRecord::new(&tx.source_account, tx.fee_charged);
            record.successful = tx.successful;

            for op in &tx.operations {
                if let Some(dest) = &op.destination {
                    record.add_destination(dest.as_str());
                }
                if let Some(src) = &op.source_account {
                    if src != &tx.source_account {
                        // Additional source accounts also participate
                        record.add_destination(src.as_str());
                    }
                }
            }

            record
        })
        .collect()
}

/// Count unique participants across all transaction records.
fn count_unique_participants(records: &[TxRecord]) -> usize {
    use std::collections::HashSet;
    let mut accounts: HashSet<&str> = HashSet::new();
    for r in records {
        accounts.insert(&r.source);
        for d in &r.destinations {
            accounts.insert(d.as_str());
        }
    }
    accounts.len()
}
