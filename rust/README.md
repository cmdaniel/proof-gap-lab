# proofgap (Rust)

Rust implementation of the proof-gap lab. See the [root README](../README.md) for the experiment, results and the comparison with Python.

```bash
cargo test --release
cargo run --release --bin experiment
cargo run --release --bin bench
```

The first build downloads Z3 5.4.1 from GitHub releases (pinned in `.cargo/config.toml`).
