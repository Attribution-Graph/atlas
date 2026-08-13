//! # ingestion
//!
//! Stellar Horizon API client for ingesting ledger transactions and operations.
//!
//! ## Overview
//!
//! This crate provides:
//! - Typed Rust models for Stellar transactions, operations, and participants
//! - An async HTTP client for the Stellar Horizon REST API
//! - Utilities for extracting participants from transaction data
//!
//! ## Example
//!
//! ```no_run
//! use ingestion::horizon::{HorizonClient, HorizonConfig};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = HorizonClient::new(HorizonConfig::testnet())?;
//!     let transactions = client.fetch_transactions_in_range(1000, 1010).await?;
//!     println!("Fetched {} transactions", transactions.len());
//!     Ok(())
//! }
//! ```

pub mod horizon;
pub mod models;

pub use horizon::{extract_participants, HorizonClient, HorizonConfig};
pub use models::{AccountId, LedgerSequence, Operation, OperationType, Participant, Transaction};
