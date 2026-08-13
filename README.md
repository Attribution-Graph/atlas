# Atlas

**Atlas** is a Stellar ledger ingestion, transaction tracing, and contribution graph analytics platform. It polls the Stellar Horizon API, builds a contribution graph from transaction data, and applies a PageRank-style ranking algorithm to surface the most influential participants.

## Architecture

```
atlas/
├── crates/
│   ├── ingestion/    # Stellar Horizon API poller & transaction models
│   ├── graph/        # Contribution graph construction & PageRank ranking
│   └── atlas-core/   # CLI entrypoint, orchestration, and persistence
```

## Crates

### `ingestion`
Polls the Stellar Horizon REST API to fetch transactions and operations within configurable ledger ranges. Deserializes responses into typed Rust models.

### `graph`
Builds a directed contribution graph from transaction streams. Each participant is a node; each transaction creates weighted edges. Implements a PageRank-style iterative ranking algorithm.

### `atlas-core`
The main binary. Wires together ingestion and graph construction, provides a CLI via `clap`, structured logging via `tracing`, and optional PostgreSQL persistence via `sqlx`.

## Usage

```bash
# Ingest ledgers 1000–2000 and compute rankings
atlas ingest --start-ledger 1000 --end-ledger 2000

# Run with verbose logging
RUST_LOG=debug atlas ingest --start-ledger 1000 --end-ledger 2000

# Persist results to Postgres
atlas ingest --start-ledger 1000 --end-ledger 2000 --database-url postgres://user:pass@localhost/atlas
```

## Development

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all
```

## License

MIT OR Apache-2.0
