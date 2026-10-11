# rchain-observer

**Multi-node observation, finalized-state cross-checking, and divergence detection for RChain networks.**

[Website: **rholox.com**](https://rholox.com) · Public testnet · Rust · Read-only observation layer

`rchain-observer` is the network evidence layer being built for the Rholox developer surface. It queries multiple RChain HTTP endpoints concurrently and returns an explicit verdict instead of silently trusting one gateway.

The larger goal is not another block explorer. It is a developer path where a DApp can **execute on RChain, interact through Rholox, and independently check what the network actually finalized**.

```text
DApp UI / service
      │
      ├── signed writes ───────────────► RChain node / Rholang runtime
      │                                  │
      │                                  └── RSpace state transition
      │
      ├── exploratory reads ───────────► node HTTP API
      │
      └── finality / agreement checks ─► rchain-observer
                                           │
                                           ├── node A
                                           ├── node B
                                           └── node C
                                                │
                                                ▼
                                 agreement / majority / divergence
```

> **Execution and observation are deliberately separate.** Rholang contracts execute on RChain. `rchain-observer` does not execute contracts and does not mutate the chain; it gives applications a multi-node view of network and finalized state.

---

## Why this exists

A DApp that reads one RPC endpoint has one view of the chain. That is convenient, but it quietly turns one HTTP server into an authority.

`rchain-observer` makes that dependency visible. For every configured node it records:

- reachability;
- network and shard identity;
- latest observed block height;
- latest finalized block height;
- finalized block hash;
- request failures;
- an explicit cross-node verdict.

The first verdict layer classifies exact finalized snapshots as:

| Verdict | Meaning |
|---|---|
| `agreement` | every comparable node reports the same finalized identity |
| `height_skew` | finalized heights differ, but no same-height hash conflict was observed; ancestry is not yet established |
| `majority` | a strict majority shares one finalized identity while a same-height conflicting hash is present |
| `divergence` | network/shard identity conflicts, or finalized hashes conflict without a strict majority |
| `unavailable` | no comparable finalized snapshot was returned |

A lagging node is evidence of a different observation. It is **not**, by itself, evidence of a consensus safety failure. `height_skew` is therefore deliberately separate from same-height hash conflict. Ancestry-aware comparison remains the next observer layer.

---

# Building DApps on Rholox / RChain

If Ethereum asks you to learn **Solidity + EVM semantics**, and TON asks you to learn **Tact/FunC + TVM/message semantics**, the corresponding execution language here is **Rholang**.

Rholox is intended to make the surrounding work—wallets, signing, deploy submission, exploration, proofs, assets, finality checks, and evidence—less fragmented. It does **not** replace Rholang.

## The mental model

| Ecosystem | Contract language | Execution model | State / interaction model | Typical client layer |
|---|---|---|---|---|
| Ethereum | Solidity | EVM | storage + calls/transactions | JS/TS + JSON-RPC |
| TON | Tact / FunC | TVM | cells + asynchronous messages | JS/TS + TON APIs |
| **RChain / Rholox** | **Rholang** | **RSpace / rho-calculus** | **names, channels, sends, receives, processes** | **JS/TS + RChain HTTP APIs + Observer** |

The important shift is that Rholang is not primarily an object-with-storage language. Its core vocabulary is concurrent process interaction.

A practical Rholox DApp therefore has two halves:

```text
contract / state logic       client / product logic
────────────────────────     ─────────────────────────────
Rholang                      TypeScript / JavaScript
RSpace channels              wallet + signing
system processes             HTTP API integration
capability boundaries        deploy lifecycle
state transitions            inclusion/finality UX
                             observer evidence
```

## What a developer should know

### 1. Rholang

This is the contract language that executes on RChain. The minimum useful concepts are:

- `new` — create fresh/private names;
- `!` — send data on a channel;
- `for` — receive and pattern-match data;
- parallel composition with `|`;
- contracts and persistent behavior;
- quoted names and unforgeable names;
- pattern matching;
- public system processes / URNs;
- capability-oriented design;
- the difference between a deploy-time effect and an exploratory read.

A tiny exploratory term can be as small as:

```rholang
new return in {
  return!("hello from Rholang")
}
```

For application contracts, the important skill is not memorizing syntax. It is learning to model authority and state as **who possesses which name/capability, what can be sent to it, and which continuation is allowed to consume the message**.

### 2. RSpace and the concurrency model

RSpace is the state substrate behind Rholang execution. A useful developer-level model is:

```text
send(data, channel)
        +
receive(pattern, channel)
        │
        ▼
     reduction
        │
        ▼
new persisted state / continuation / output
```

This matters because contract design is shaped by communication and matching, not by pretending the system is a single-threaded key/value store.

### 3. RChain system processes

Real applications usually interact with installed system facilities rather than rebuilding everything from first principles. Important names include facilities for:

- registry lookup;
- REV vault access and address handling;
- deploy identity;
- cryptographic hashing and verification;
- MakeMint / asset primitives;
- other installed contracts exposed by the network.

Two details are especially important:

1. `rho:rchain:deployId` / `deployerId` exist on the real deploy path.
2. They are **not available under exploratory deploy**, so a term that depends on deploy identity cannot be treated as an explore-deploy test without adaptation.

### 4. TypeScript / JavaScript for the client layer

Rholang is the execution language. A browser or service still needs a client language. Rholox currently uses TypeScript for this layer.

The current Rholox implementation already demonstrates:

- secp256k1 key generation/import;
- REV address derivation;
- deploy serialization and signing;
- `blake2b` deploy hashing;
- signed submission to `/api/deploy`;
- polling `/api/v1/deploy-status/{deployId}`;
- exploratory reads through `/api/explore-deploy`;
- block and finalized-state inspection;
- proof anchoring;
- REV transfer flows;
- fixed-supply assets;
- capability-custodied testnet exchange mechanics.

Today those primitives live in the Rholox application code. The direction is to make the reusable boundaries progressively easier for independent DApps to consume without forcing every application to rebuild signing, decoding, finality handling, and observation logic.

### 5. Deploy signing

A write is not just “send this Rholang string.” A client constructs deploy data including the term and chain context, signs the serialized deploy with the wallet key, and submits the signed envelope.

The current Rholox path uses:

```text
Rholang term
   │
   ├── timestamp
   ├── phlo price / limit
   ├── valid-after block
   └── shard id
        │
        ▼
serialized deploy data
        │
   blake2b-256
        │
  secp256k1 signature
        │
        ▼
POST /api/deploy
```

A DApp author should understand this lifecycle even when a library eventually hides most of the serialization details.

### 6. Inclusion is not finality

This is one of the most important rules in the stack.

```text
submitted
   ↓
accepted / pooled
   ↓
processed
   ↓
included in a block
   ↓
finalized state
```

Those are different states.

A good client should never turn “the node returned a deploy id” into “the action is final.” Rholox already separates submission, processing, inclusion, and finalized read-back. `rchain-observer` extends that model from **one endpoint's answer** toward **cross-node finalized-state evidence**.

---

# The Rholox developer architecture

The intended stack is:

```text
┌──────────────────────────────────────────────────────────────┐
│                         DApp                                 │
│  React / TS, mobile, CLI, service, game, exchange, tool    │
└──────────────┬───────────────────────────────┬───────────────┘
               │                               │
               │ signed deploy / reads         │ evidence
               ▼                               ▼
┌────────────────────────────┐     ┌───────────────────────────┐
│       Rholox surface       │     │      rchain-observer      │
│ wallet · deploy · proof    │     │ multi-node observation    │
│ explorer · assets · REV    │     │ finality cross-checking   │
└──────────────┬─────────────┘     └─────────────┬─────────────┘
               │                                 │
               └──────────────┬──────────────────┘
                              ▼
                    ┌──────────────────┐
                    │   RChain nodes   │
                    │ HTTP + Rholang   │
                    └────────┬─────────┘
                             ▼
                    ┌──────────────────┐
                    │ RSpace / Casper  │
                    │ finalized state  │
                    └──────────────────┘
```

### What executes where

| Component | Responsibility |
|---|---|
| **Rholang contract** | state transition, authority, on-chain behavior |
| **RChain node** | normalization, execution, block inclusion, finalized state |
| **Rholox client layer** | wallet, signing, submission, reads, product UX |
| **rchain-observer** | independent read-only cross-node observation and verdicts |
| **your DApp** | domain logic and user experience built on those boundaries |

The observer is therefore not a replacement for a node and not a contract runtime. It is the piece that answers a different question:

> “Do the nodes I chose to trust as independent observation points agree on the finalized state I am about to present to my users?”

---

# A DApp transaction lifecycle

A production-shaped Rholox application should think in this order:

```text
1. Build Rholang term
2. Preflight read-only paths where possible
3. Construct deploy data from live node status
4. Sign locally
5. Submit signed deploy
6. Track processing status
7. Capture the inclusion block
8. Read the resulting state back
9. Wait for finalized-state visibility
10. Cross-check observation across multiple nodes
11. Present final state to the user
```

The write path and evidence path stay separate:

```text
WRITE
wallet → signed deploy → node → Rholang → RSpace

EVIDENCE
node A ─┐
node B ─┼→ rchain-observer → verdict → DApp
node C ─┘
```

That separation is intentional. The same component that broadcasts a transaction should not automatically be treated as the only authority on whether the resulting state is final.

For the full developer walkthrough, see **[Building DApps on Rholox / RChain](docs/BUILDING-DAPPS.md)**.

---

# Observer API

## Run once

```bash
cargo run -- \
  --node gateway=https://testnet.rhobot.net \
  --node validator-a=https://validator-a.example \
  --node validator-b=https://validator-b.example \
  once
```

The command emits one JSON evidence report containing every node observation and the derived verdict.

## Serve as an API

```bash
cargo run -- \
  --node gateway=https://testnet.rhobot.net \
  --node validator-a=https://validator-a.example \
  serve --listen 127.0.0.1:8088 --refresh-ms 5000
```

Endpoints:

```text
GET /health
GET /v1/observation
```

The observer currently reads these node surfaces:

```text
GET /api/status
GET /api/last-finalized-block
```

Example application flow:

```ts
const report = await fetch('http://127.0.0.1:8088/v1/observation')
  .then((response) => response.json())

if (report.verdict.class === 'divergence') {
  // Degrade or block claims that depend on a single finalized view.
}
```

Do not reduce the verdict to a green/red decoration. Preserve the underlying observations so the application can explain **which nodes disagreed, at what height, and on which finalized hash**.

---

# Current boundary

`rchain-observer` is intentionally read-only.

It does **not**:

- submit deploys;
- propose blocks;
- hold wallet keys;
- manage validator keys;
- mutate RChain state;
- execute Rholang;
- claim that a different height automatically means a consensus fault;
- turn incomplete observations into a safety theorem.

The current exact-snapshot classifier is a first layer. Planned evolution includes ancestry-aware finality comparison, bounded observation history, stable evidence identities, configurable quorum policy, and direct Rholox consumption.

The public Rholox interface currently exists at **https://rholox.com**. Its present client path still talks to the public testnet node surface directly; `rchain-observer` is being built as the multi-node evidence boundary that removes the need for application surfaces to silently rely on a single endpoint.

---

# Repository layout

```text
src/main.rs       CLI + HTTP service
src/observer.rs   concurrent node observation
src/model.rs      observation/report/verdict data model
src/verdict.rs    exact finalized-snapshot classifier
tests/verdict.rs  deterministic verdict tests

docs/BUILDING-DAPPS.md  DApp developer guide
examples/hello.rho       minimal Rholang explore example
```

---

# Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Run the minimal Rholang example with an RChain exploratory-deploy client against a compatible public testnet endpoint; the file is intentionally tiny so the transport/result semantics are visible instead of hidden behind application code.

---

# Near-term engineering path

1. **Distinguish lag from finalized-state divergence** with ancestry-aware comparison.
2. **Persist bounded observation history** so applications can reason about transitions, not only snapshots.
3. **Stable evidence identities** for observation reports.
4. **Configurable quorum policy** for different operational environments.
5. **Rholox integration** so Network Console and DApps can consume multi-node verdicts.
6. **Reusable client boundaries** for signing, deploy lifecycle, decoding, finality, and observation.
7. **DApp templates** that start with Rholang + TypeScript + evidence rather than rebuilding the entire stack.

---

## One sentence

**Write state transitions in Rholang, drive them from TypeScript, execute them on RChain, and use Rholox + `rchain-observer` to make the resulting network evidence explicit.**
