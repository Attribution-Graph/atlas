//! Structured logging and telemetry setup for Atlas.
//!
//! Initializes the `tracing` subscriber with:
//! - Human-readable output on stdout (via `tracing-subscriber` fmt layer)
//! - `RUST_LOG` env-var support for per-module level filtering
//! - A `--verbose` CLI flag that escalates the default log level
//!
//! # Usage
//!
//! ```no_run
//! use atlas_core::telemetry;
//!
//! telemetry::init(2); // 0=info, 1=debug, 2+=trace
//! tracing::info!("Atlas started");
//! tracing::debug!(ledger = 1000, "Processing ledger");
//! ```

use tracing::Level;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize the global tracing subscriber.
///
/// `verbosity` maps to log levels:
/// - 0 → `INFO`
/// - 1 → `DEBUG`
/// - 2+ → `TRACE`
///
/// The `RUST_LOG` environment variable always takes precedence if set,
/// allowing per-crate/module level overrides (e.g. `RUST_LOG=atlas=debug,reqwest=warn`).
pub fn init(verbosity: u8) {
    let default_level = match verbosity {
        0 => Level::INFO,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    };

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level.as_str()));

    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false);

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(env_filter)
        .init();
}

/// Initialize a compact tracing subscriber for tests.
///
/// Uses a more compact format without timestamps, suitable for test output.
/// This is a no-op if a subscriber is already initialized.
#[allow(dead_code)]
pub fn init_test() {
    let _ = tracing_subscriber::fmt()
        .compact()
        .with_test_writer()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug")),
        )
        .try_init();
}

/// A span guard for tracking the duration of a named operation.
///
/// # Example
/// ```no_run
/// use atlas_core::telemetry::OperationSpan;
///
/// let _span = OperationSpan::new("ingest", "ledger_range", "[1000..2000]");
/// // ... do work ...
/// // span is automatically closed when dropped
/// ```
#[allow(dead_code)]
pub struct OperationSpan {
    _inner: tracing::span::EnteredSpan,
}

impl OperationSpan {
    /// Create and enter a new named span.
    #[allow(dead_code)]
    pub fn new(operation: &str, key: &str, value: &str) -> Self {
        let span = tracing::info_span!("operation", %operation, %key, %value);
        Self {
            _inner: span.entered(),
        }
    }
}
