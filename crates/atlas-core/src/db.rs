//! PostgreSQL persistence layer for Atlas ingestion results.
//!
//! Provides functions to persist ranking results and transaction metadata
//! to a PostgreSQL database via `sqlx`.

use anyhow::{Context, Result};
use tracing::{debug, info};

use crate::orchestrator::IngestResult;

/// Persist an ingestion result to the given PostgreSQL database.
///
/// Creates the required tables if they do not exist, then inserts
/// the ranking entries for the run.
pub async fn persist(result: &IngestResult, database_url: &str) -> Result<()> {
    let pool = sqlx::PgPool::connect(database_url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    info!("Connected to database, running migrations");
    run_migrations(&pool).await?;

    persist_run(&pool, result).await?;
    info!("Persistence complete");
    Ok(())
}

/// Run inline schema migrations.
async fn run_migrations(pool: &sqlx::PgPool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS atlas_runs (
            id          SERIAL PRIMARY KEY,
            start_ledger INTEGER NOT NULL,
            end_ledger   INTEGER NOT NULL,
            tx_count     INTEGER NOT NULL,
            participant_count INTEGER NOT NULL,
            ran_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create atlas_runs table")?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS atlas_rankings (
            id          SERIAL PRIMARY KEY,
            run_id      INTEGER NOT NULL REFERENCES atlas_runs(id) ON DELETE CASCADE,
            account_id  TEXT NOT NULL,
            score       DOUBLE PRECISION NOT NULL,
            rank        INTEGER NOT NULL,
            in_degree   INTEGER NOT NULL,
            out_degree  INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_rankings_run_id ON atlas_rankings(run_id);
        CREATE INDEX IF NOT EXISTS idx_rankings_score ON atlas_rankings(score DESC);
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create atlas_rankings table")?;

    Ok(())
}

/// Insert a run and its ranking entries into the database.
async fn persist_run(pool: &sqlx::PgPool, result: &IngestResult) -> Result<()> {
    let run_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO atlas_runs (start_ledger, end_ledger, tx_count, participant_count)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(result.start_ledger as i32)
    .bind(result.end_ledger as i32)
    .bind(result.transaction_count as i32)
    .bind(result.participant_count as i32)
    .fetch_one(pool)
    .await
    .context("Failed to insert run record")?;

    debug!(run_id, "Inserted run record");

    // Batch-insert ranking entries
    for entry in &result.ranking.entries {
        sqlx::query(
            r#"
            INSERT INTO atlas_rankings (run_id, account_id, score, rank, in_degree, out_degree)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(run_id)
        .bind(&entry.account_id)
        .bind(entry.score)
        .bind(entry.rank as i32)
        .bind(entry.in_degree as i32)
        .bind(entry.out_degree as i32)
        .execute(pool)
        .await
        .context("Failed to insert ranking entry")?;
    }

    info!(
        run_id,
        entries = result.ranking.entries.len(),
        "Ranking entries persisted"
    );
    Ok(())
}
