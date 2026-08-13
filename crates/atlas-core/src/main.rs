//! Atlas — Stellar ledger ingestion and contribution graph analytics.
//!
//! # Usage
//!
//! ```bash
//! # Ingest ledgers 1000–2000 and compute contribution rankings
//! atlas ingest --start-ledger 1000 --end-ledger 2000
//!
//! # Output rankings as JSON
//! atlas ingest --start-ledger 1000 --end-ledger 2000 --output json
//!
//! # Use testnet
//! atlas ingest --start-ledger 1000 --end-ledger 2000 --network testnet
//!
//! # Persist results to PostgreSQL
//! atlas ingest --start-ledger 1000 --end-ledger 2000 \
//!     --database-url postgres://user:pass@localhost/atlas
//! ```

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use tracing::{info, warn};

mod db;
mod orchestrator;

use orchestrator::IngestConfig;

/// Atlas: Stellar ledger ingestion and contribution graph analytics.
#[derive(Parser, Debug)]
#[command(name = "atlas", version, about, long_about = None)]
struct Cli {
    /// Logging verbosity. Set RUST_LOG for fine-grained control.
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Ingest Stellar ledgers and compute contribution rankings.
    Ingest(IngestArgs),
}

/// Arguments for the `ingest` subcommand.
#[derive(Parser, Debug)]
struct IngestArgs {
    /// Starting ledger sequence number (inclusive).
    #[arg(long, value_name = "LEDGER")]
    start_ledger: u32,

    /// Ending ledger sequence number (inclusive).
    #[arg(long, value_name = "LEDGER")]
    end_ledger: u32,

    /// Stellar network to target.
    #[arg(long, default_value = "mainnet")]
    network: Network,

    /// Custom Horizon base URL (overrides --network).
    #[arg(long, value_name = "URL")]
    horizon_url: Option<String>,

    /// Output format for rankings.
    #[arg(long, default_value = "table")]
    output: OutputFormat,

    /// PostgreSQL connection URL for persisting results.
    #[arg(long, value_name = "URL", env = "DATABASE_URL")]
    database_url: Option<String>,

    /// Number of top-ranked participants to display.
    #[arg(long, default_value = "20")]
    top: usize,

    /// PageRank damping factor.
    #[arg(long, default_value = "0.85")]
    damping: f64,

    /// Maximum PageRank iterations.
    #[arg(long, default_value = "100")]
    max_iterations: u32,
}

/// Target Stellar network.
#[derive(ValueEnum, Clone, Debug)]
enum Network {
    Mainnet,
    Testnet,
}

/// Output format for rankings.
#[derive(ValueEnum, Clone, Debug)]
enum OutputFormat {
    Table,
    Json,
    Csv,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize structured logging
    let log_level = match cli.verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    init_tracing(log_level);

    info!(version = env!("CARGO_PKG_VERSION"), "Atlas starting");

    match cli.command {
        Commands::Ingest(args) => run_ingest(args).await,
    }
}

async fn run_ingest(args: IngestArgs) -> Result<()> {
    // Validate ledger range
    if args.start_ledger > args.end_ledger {
        anyhow::bail!(
            "start-ledger ({}) must be <= end-ledger ({})",
            args.start_ledger,
            args.end_ledger
        );
    }

    info!(
        start_ledger = args.start_ledger,
        end_ledger = args.end_ledger,
        network = ?args.network,
        "Starting ingestion"
    );

    // Build Horizon client config
    let horizon_url = match (&args.horizon_url, &args.network) {
        (Some(url), _) => url.clone(),
        (None, Network::Testnet) => ingestion::HorizonConfig::testnet().base_url,
        (None, Network::Mainnet) => ingestion::HorizonConfig::default().base_url,
    };

    let config = IngestConfig {
        horizon_url,
        start_ledger: args.start_ledger,
        end_ledger: args.end_ledger,
        damping: args.damping,
        max_iterations: args.max_iterations,
        database_url: args.database_url.clone(),
        top_n: args.top,
    };

    let result = orchestrator::run(config).await?;

    // Output results
    match args.output {
        OutputFormat::Table => print_table(&result, args.top),
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&result.ranking)
                .context("Failed to serialize ranking to JSON")?;
            println!("{}", json);
        }
        OutputFormat::Csv => print_csv(&result),
    }

    // Persist to database if requested
    if let Some(db_url) = &args.database_url {
        info!(url = %db_url, "Persisting results to database");
        match db::persist(&result, db_url).await {
            Ok(_) => info!("Results persisted successfully"),
            Err(e) => warn!(error = %e, "Failed to persist results"),
        }
    }

    Ok(())
}

fn print_table(result: &orchestrator::IngestResult, top_n: usize) {
    println!(
        "\n{:=<60}",
        format!(" Atlas Rankings (ledgers {}-{}) ", result.start_ledger, result.end_ledger)
    );
    println!("{:<6} {:<56} {:>10}", "Rank", "Account ID", "Score");
    println!("{:-<75}", "");

    for entry in result.ranking.top(top_n) {
        let display = if entry.account_id.len() > 54 {
            format!("{}…", &entry.account_id[..53])
        } else {
            entry.account_id.clone()
        };
        println!("{:<6} {:<56} {:>10.6}", entry.rank + 1, display, entry.score);
    }

    println!("{:-<75}", "");
    println!(
        "Total participants: {} | Transactions: {} | Iterations: {} | Converged: {}",
        result.ranking.entries.len(),
        result.transaction_count,
        result.ranking.iterations,
        result.ranking.converged,
    );
}

fn print_csv(result: &orchestrator::IngestResult) {
    println!("rank,account_id,score,in_degree,out_degree");
    for entry in &result.ranking.entries {
        println!(
            "{},{},{:.8},{},{}",
            entry.rank + 1,
            entry.account_id,
            entry.score,
            entry.in_degree,
            entry.out_degree
        );
    }
}

fn init_tracing(default_level: &str) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(true))
        .with(env_filter)
        .init();
}
