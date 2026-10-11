# rchain-observer

Multi-node observation, finalized-state cross-checking, and divergence detection for RChain networks.

`rchain-observer` queries multiple RChain HTTP endpoints concurrently and returns an explicit verdict instead of silently trusting a single endpoint.

## What it checks

For every configured node it observes:

- network and shard identity;
- latest observed block height;
- latest finalized block height;
- finalized block hash;
- reachability and request failures.

The first verdict layer compares exact finalized snapshots across all comparable nodes:

- `agreement` — every comparable node reports the same finalized identity;
- `majority` — a strict majority agrees and at least one node differs;
- `divergence` — there is no strict majority;
- `unavailable` — no comparable finalized snapshot was returned.

A lagging node is evidence of a different observation, not by itself evidence of a consensus safety failure. Ancestry-aware finality comparison belongs in the next layer.

## Run once

```bash
cargo run -- \
  --node gateway=https://testnet.rhobot.net \
  --node validator-a=https://validator-a.example \
  --node validator-b=https://validator-b.example \
  once
```

The command emits one JSON evidence report containing every node observation and the derived verdict.

## Serve the observation API

```bash
cargo run -- \
  --node gateway=https://testnet.rhobot.net \
  --node validator-a=https://validator-a.example \
  serve --listen 127.0.0.1:8088
```

Endpoints:

- `GET /health`
- `GET /v1/observation`

This API is intended to be small enough for explorers, dashboards, testnet operations, and application surfaces to consume without duplicating node-comparison logic.

## Design boundary

The observer is read-only. It does not propose blocks, submit deploys, manage validator keys, or mutate node state.

The current verdict is deliberately narrow: it reports what the configured endpoints returned and classifies exact finalized-state agreement. It does not infer consensus safety from incomplete observations.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## Near-term work

1. ancestry-aware comparison for nodes at different finalized heights;
2. signed evidence envelopes and stable observation IDs;
3. bounded observation history and divergence timelines;
4. configurable quorum policy;
5. Rholox integration against the observation API.
