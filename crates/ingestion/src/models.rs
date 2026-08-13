//! Core data models for Stellar transactions and operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A Stellar account identifier (public key).
pub type AccountId = String;

/// A Stellar ledger sequence number.
pub type LedgerSequence = u32;

/// Represents a single Stellar transaction fetched from Horizon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Unique identifier for this transaction record.
    pub id: Uuid,
    /// The transaction hash on the Stellar network.
    pub hash: String,
    /// The ledger this transaction was included in.
    pub ledger: LedgerSequence,
    /// Timestamp when the transaction was created.
    pub created_at: DateTime<Utc>,
    /// The source account that initiated the transaction.
    pub source_account: AccountId,
    /// Operations included in this transaction.
    pub operations: Vec<Operation>,
    /// Fee charged for this transaction (in stroops).
    pub fee_charged: u64,
    /// Whether the transaction succeeded.
    pub successful: bool,
}

impl Transaction {
    /// Create a new transaction with a generated UUID.
    pub fn new(
        hash: String,
        ledger: LedgerSequence,
        created_at: DateTime<Utc>,
        source_account: AccountId,
        fee_charged: u64,
        successful: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            hash,
            ledger,
            created_at,
            source_account,
            operations: Vec::new(),
            fee_charged,
            successful,
        }
    }

    /// Add an operation to this transaction.
    pub fn add_operation(&mut self, op: Operation) {
        self.operations.push(op);
    }
}

/// The type of a Stellar operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationType {
    CreateAccount,
    Payment,
    PathPaymentStrictReceive,
    PathPaymentStrictSend,
    ManageSellOffer,
    ManageBuyOffer,
    CreatePassiveSellOffer,
    SetOptions,
    ChangeTrust,
    AllowTrust,
    AccountMerge,
    Inflation,
    ManageData,
    BumpSequence,
    CreateClaimableBalance,
    ClaimClaimableBalance,
    BeginSponsoringFutureReserves,
    EndSponsoringFutureReserves,
    RevokeSponsorship,
    Clawback,
    ClawbackClaimableBalance,
    SetTrustLineFlags,
    LiquidityPoolDeposit,
    LiquidityPoolWithdraw,
    Unknown,
}

impl From<&str> for OperationType {
    fn from(s: &str) -> Self {
        match s {
            "create_account" => Self::CreateAccount,
            "payment" => Self::Payment,
            "path_payment_strict_receive" => Self::PathPaymentStrictReceive,
            "path_payment_strict_send" => Self::PathPaymentStrictSend,
            "manage_sell_offer" => Self::ManageSellOffer,
            "manage_buy_offer" => Self::ManageBuyOffer,
            "create_passive_sell_offer" => Self::CreatePassiveSellOffer,
            "set_options" => Self::SetOptions,
            "change_trust" => Self::ChangeTrust,
            "allow_trust" => Self::AllowTrust,
            "account_merge" => Self::AccountMerge,
            "inflation" => Self::Inflation,
            "manage_data" => Self::ManageData,
            "bump_sequence" => Self::BumpSequence,
            "create_claimable_balance" => Self::CreateClaimableBalance,
            "claim_claimable_balance" => Self::ClaimClaimableBalance,
            "begin_sponsoring_future_reserves" => Self::BeginSponsoringFutureReserves,
            "end_sponsoring_future_reserves" => Self::EndSponsoringFutureReserves,
            "revoke_sponsorship" => Self::RevokeSponsorship,
            "clawback" => Self::Clawback,
            "clawback_claimable_balance" => Self::ClawbackClaimableBalance,
            "set_trust_line_flags" => Self::SetTrustLineFlags,
            "liquidity_pool_deposit" => Self::LiquidityPoolDeposit,
            "liquidity_pool_withdraw" => Self::LiquidityPoolWithdraw,
            _ => Self::Unknown,
        }
    }
}

/// A single Stellar operation within a transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    /// Unique identifier for this operation record.
    pub id: Uuid,
    /// The operation's Horizon ID.
    pub horizon_id: String,
    /// The type of operation.
    pub op_type: OperationType,
    /// The source account for this operation (may differ from tx source).
    pub source_account: Option<AccountId>,
    /// The destination account, if applicable (e.g., payment destination).
    pub destination: Option<AccountId>,
    /// The amount transacted, if applicable (in stroops or base units).
    pub amount: Option<f64>,
    /// The asset code, if applicable.
    pub asset_code: Option<String>,
    /// The asset issuer, if applicable.
    pub asset_issuer: Option<AccountId>,
}

impl Operation {
    /// Create a new operation with a generated UUID.
    pub fn new(horizon_id: String, op_type: OperationType) -> Self {
        Self {
            id: Uuid::new_v4(),
            horizon_id,
            op_type,
            source_account: None,
            destination: None,
            amount: None,
            asset_code: None,
            asset_issuer: None,
        }
    }
}

/// A participant in the Stellar network, identified by account ID.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Participant {
    /// The Stellar account ID (public key).
    pub account_id: AccountId,
    /// Human-readable display name, if known.
    pub display_name: Option<String>,
}

impl Participant {
    /// Create a participant from an account ID.
    pub fn new(account_id: impl Into<AccountId>) -> Self {
        Self {
            account_id: account_id.into(),
            display_name: None,
        }
    }

    /// Create a participant with a display name.
    pub fn with_name(account_id: impl Into<AccountId>, name: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            display_name: Some(name.into()),
        }
    }
}

/// Raw Horizon API response envelope for paginated record lists.
#[derive(Debug, Deserialize)]
pub struct HorizonPage<T> {
    #[serde(rename = "_embedded")]
    pub embedded: HorizonEmbedded<T>,
    #[serde(rename = "_links")]
    pub links: HorizonLinks,
}

#[derive(Debug, Deserialize)]
pub struct HorizonEmbedded<T> {
    pub records: Vec<T>,
}

#[derive(Debug, Deserialize)]
pub struct HorizonLinks {
    pub next: Option<HorizonLink>,
    pub prev: Option<HorizonLink>,
    #[serde(rename = "self")]
    pub self_link: Option<HorizonLink>,
}

#[derive(Debug, Deserialize)]
pub struct HorizonLink {
    pub href: String,
}

/// Raw Horizon transaction record as returned by the API.
#[derive(Debug, Deserialize)]
pub struct HorizonTransaction {
    pub hash: String,
    pub ledger: LedgerSequence,
    pub created_at: String,
    pub source_account: String,
    pub fee_charged: String,
    pub successful: bool,
    pub operation_count: u32,
}

/// Raw Horizon operation record as returned by the API.
#[derive(Debug, Deserialize)]
pub struct HorizonOperation {
    pub id: String,
    #[serde(rename = "type")]
    pub op_type: String,
    pub source_account: Option<String>,
    pub to: Option<String>,
    pub from: Option<String>,
    pub amount: Option<String>,
    pub asset_code: Option<String>,
    pub asset_issuer: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_creation() {
        let now = Utc::now();
        let mut tx = Transaction::new(
            "abc123".to_string(),
            1000,
            now,
            "GABC".to_string(),
            100,
            true,
        );
        assert_eq!(tx.hash, "abc123");
        assert_eq!(tx.ledger, 1000);
        assert!(tx.operations.is_empty());

        let op = Operation::new("op1".to_string(), OperationType::Payment);
        tx.add_operation(op);
        assert_eq!(tx.operations.len(), 1);
    }

    #[test]
    fn test_operation_type_from_str() {
        assert_eq!(OperationType::from("payment"), OperationType::Payment);
        assert_eq!(OperationType::from("create_account"), OperationType::CreateAccount);
        assert_eq!(OperationType::from("unknown_op"), OperationType::Unknown);
    }

    #[test]
    fn test_participant_creation() {
        let p = Participant::new("GABC123");
        assert_eq!(p.account_id, "GABC123");
        assert!(p.display_name.is_none());

        let p2 = Participant::with_name("GDEF456", "Alice");
        assert_eq!(p2.display_name, Some("Alice".to_string()));
    }
}
