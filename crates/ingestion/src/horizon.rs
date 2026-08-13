//! Stellar Horizon API client for fetching transactions and operations.

use crate::models::{
    AccountId, HorizonOperation, HorizonPage, HorizonTransaction, LedgerSequence, Operation,
    OperationType, Transaction,
};
use anyhow::{Context, Result};
use chrono::DateTime;
use tracing::{debug, info, warn};

/// Default Horizon mainnet base URL.
pub const HORIZON_MAINNET_URL: &str = "https://horizon.stellar.org";

/// Default Horizon testnet base URL.
pub const HORIZON_TESTNET_URL: &str = "https://horizon-testnet.stellar.org";

/// Configuration for the Horizon client.
#[derive(Debug, Clone)]
pub struct HorizonConfig {
    /// Base URL of the Horizon server.
    pub base_url: String,
    /// Maximum records per page when paginating.
    pub page_limit: u32,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for HorizonConfig {
    fn default() -> Self {
        Self {
            base_url: HORIZON_MAINNET_URL.to_string(),
            page_limit: 200,
            timeout_secs: 30,
        }
    }
}

impl HorizonConfig {
    /// Create a config targeting the Stellar testnet.
    pub fn testnet() -> Self {
        Self {
            base_url: HORIZON_TESTNET_URL.to_string(),
            ..Default::default()
        }
    }

    /// Create a config with a custom base URL.
    pub fn with_url(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            ..Default::default()
        }
    }
}

/// Horizon API client for ingesting Stellar data.
pub struct HorizonClient {
    http: reqwest::Client,
    config: HorizonConfig,
}

impl HorizonClient {
    /// Create a new client with the given configuration.
    pub fn new(config: HorizonConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .user_agent("atlas-ingestion/0.1.0")
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self { http, config })
    }

    /// Create a client targeting the Stellar mainnet with default settings.
    pub fn mainnet() -> Result<Self> {
        Self::new(HorizonConfig::default())
    }

    /// Create a client targeting the Stellar testnet.
    pub fn testnet() -> Result<Self> {
        Self::new(HorizonConfig::testnet())
    }

    /// Fetch all transactions in a given ledger range [start, end] (inclusive).
    ///
    /// Returns a flat list of [`Transaction`] objects with their operations populated.
    pub async fn fetch_transactions_in_range(
        &self,
        start_ledger: LedgerSequence,
        end_ledger: LedgerSequence,
    ) -> Result<Vec<Transaction>> {
        info!(
            start_ledger,
            end_ledger, "Fetching transactions in ledger range"
        );

        let mut all_transactions = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let url = self.build_transactions_url(cursor.as_deref());
            debug!(url = %url, "Fetching transactions page");

            let page: HorizonPage<HorizonTransaction> = self
                .http
                .get(&url)
                .send()
                .await
                .context("HTTP request failed")?
                .error_for_status()
                .context("Horizon API returned error status")?
                .json()
                .await
                .context("Failed to deserialize Horizon response")?;

            let records = page.embedded.records;
            if records.is_empty() {
                break;
            }

            let last_in_range = records
                .iter()
                .filter(|tx| tx.ledger >= start_ledger && tx.ledger <= end_ledger)
                .count();

            debug!(
                page_size = records.len(),
                in_range = last_in_range,
                "Processing transaction page"
            );

            // Get the cursor for the next page before we filter
            let next_cursor = records.last().map(|tx| tx.hash.clone());
            let exceeded_range = records.iter().any(|tx| tx.ledger > end_ledger);

            for raw_tx in records {
                if raw_tx.ledger < start_ledger {
                    continue;
                }
                if raw_tx.ledger > end_ledger {
                    break;
                }

                match self.convert_transaction(raw_tx).await {
                    Ok(tx) => all_transactions.push(tx),
                    Err(e) => {
                        warn!(error = %e, "Failed to convert transaction, skipping");
                    }
                }
            }

            if exceeded_range {
                break;
            }

            match page.links.next {
                Some(link) if !link.href.is_empty() => {
                    // Extract cursor from next link
                    cursor = next_cursor;
                }
                _ => break,
            }
        }

        info!(
            count = all_transactions.len(),
            "Completed transaction ingestion"
        );
        Ok(all_transactions)
    }

    /// Fetch transactions for a specific ledger sequence.
    pub async fn fetch_ledger_transactions(
        &self,
        ledger: LedgerSequence,
    ) -> Result<Vec<Transaction>> {
        self.fetch_transactions_in_range(ledger, ledger).await
    }

    /// Fetch operations for a specific transaction by hash.
    pub async fn fetch_operations_for_transaction(&self, tx_hash: &str) -> Result<Vec<Operation>> {
        let url = format!(
            "{}/transactions/{}/operations?limit={}",
            self.config.base_url, tx_hash, self.config.page_limit
        );
        debug!(url = %url, "Fetching operations for transaction");

        let page: HorizonPage<HorizonOperation> = self
            .http
            .get(&url)
            .send()
            .await
            .context("HTTP request failed")?
            .error_for_status()
            .context("Horizon API returned error status")?
            .json()
            .await
            .context("Failed to deserialize operations response")?;

        let operations = page
            .embedded
            .records
            .into_iter()
            .map(convert_operation)
            .collect();

        Ok(operations)
    }

    fn build_transactions_url(&self, cursor: Option<&str>) -> String {
        let mut url = format!(
            "{}/transactions?limit={}&order=asc",
            self.config.base_url, self.config.page_limit
        );
        if let Some(c) = cursor {
            url.push_str(&format!("&cursor={}", c));
        }
        url
    }

    async fn convert_transaction(&self, raw: HorizonTransaction) -> Result<Transaction> {
        let created_at = DateTime::parse_from_rfc3339(&raw.created_at)
            .context("Failed to parse transaction timestamp")?
            .with_timezone(&chrono::Utc);

        let fee_charged: u64 = raw
            .fee_charged
            .parse()
            .context("Failed to parse fee_charged")?;

        let mut tx = Transaction::new(
            raw.hash.clone(),
            raw.ledger,
            created_at,
            raw.source_account,
            fee_charged,
            raw.successful,
        );

        // Fetch operations for this transaction
        match self.fetch_operations_for_transaction(&raw.hash).await {
            Ok(ops) => {
                for op in ops {
                    tx.add_operation(op);
                }
            }
            Err(e) => {
                warn!(
                    tx_hash = %raw.hash,
                    error = %e,
                    "Failed to fetch operations for transaction"
                );
            }
        }

        Ok(tx)
    }
}

/// Convert a raw Horizon operation record into a typed [`Operation`].
fn convert_operation(raw: HorizonOperation) -> Operation {
    let op_type = OperationType::from(raw.op_type.as_str());
    let mut op = Operation::new(raw.id, op_type);

    op.source_account = raw.source_account;
    op.destination = raw.to.or(raw.from);
    op.amount = raw.amount.and_then(|a| a.parse().ok());
    op.asset_code = raw.asset_code;
    op.asset_issuer = raw.asset_issuer;

    op
}

/// Extract all unique participant [`AccountId`]s from a list of transactions.
pub fn extract_participants(transactions: &[Transaction]) -> Vec<AccountId> {
    use std::collections::HashSet;
    let mut seen: HashSet<AccountId> = HashSet::new();

    for tx in transactions {
        seen.insert(tx.source_account.clone());
        for op in &tx.operations {
            if let Some(src) = &op.source_account {
                seen.insert(src.clone());
            }
            if let Some(dest) = &op.destination {
                seen.insert(dest.clone());
            }
        }
    }

    seen.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Operation, OperationType, Transaction};

    fn make_tx(hash: &str, ledger: u32, source: &str) -> Transaction {
        Transaction::new(
            hash.to_string(),
            ledger,
            chrono::Utc::now(),
            source.to_string(),
            100,
            true,
        )
    }

    #[test]
    fn test_extract_participants() {
        let mut tx = make_tx("hash1", 100, "ALICE");
        let mut op = Operation::new("op1".to_string(), OperationType::Payment);
        op.destination = Some("BOB".to_string());
        tx.add_operation(op);

        let participants = extract_participants(&[tx]);
        assert!(participants.contains(&"ALICE".to_string()));
        assert!(participants.contains(&"BOB".to_string()));
    }

    #[test]
    fn test_horizon_config_defaults() {
        let cfg = HorizonConfig::default();
        assert_eq!(cfg.base_url, HORIZON_MAINNET_URL);
        assert_eq!(cfg.page_limit, 200);
    }

    #[test]
    fn test_horizon_config_testnet() {
        let cfg = HorizonConfig::testnet();
        assert_eq!(cfg.base_url, HORIZON_TESTNET_URL);
    }
}
