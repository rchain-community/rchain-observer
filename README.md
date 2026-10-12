# rchain-observer

**A live, read-only, multi-node evidence layer for the RChain public testnet.**

**Provenance:** initiated and developed by [**AixAria (`@aixaria0`)**](https://github.com/aixaria0) for the [**RChain Community**](https://github.com/rchain-community).

[**Rholox**](https://rholox.com) · [**Live Observer API**](https://testnet.rhobot.net/observer/v1/observation) · [**Observer Health**](https://testnet.rhobot.net/observer/health) · Rust

The public testnet already has an execution endpoint. What it did not have was an application-facing way to answer a harder question:

> **Do the independent nodes agree on the finalized state I am about to present to a user?**

`rchain-observer` is that boundary.

It does not submit deploys, propose blocks, hold keys, or execute Rholang. It continuously reads multiple validators, preserves their individual observations, and derives an explicit verdict without turning one gateway into the definition of truth.

---

# 1. The live public testnet

The observer is connected to the **four bonded validators** of the live RChain public testnet.

```text
                         PUBLIC INTERNET
                               │
                 ┌─────────────┴─────────────┐
                 │                           │
                 ▼                           ▼
      testnet.rhobot.net            /observer/*
      execution gateway             evidence surface
                 │                           │
                 ▼                           ▼
          validator A              rchain-observer
                                             │
                           private observation paths
                                             │
                     ┌───────────┬───────────┬───────────┐
                     ▼           ▼           ▼           ▼
                     A           D           B           C
                     │           │           │           │
                     └───────────┴─────┬─────┴───────────┘
                                       ▼
                          finalized-state comparison
                                       │
                                       ▼
             agreement / proven_lag / height_skew /
         majority / conflicting_finalized_history /
                    divergence / unavailable
```

The separation is deliberate:

- **writes and normal node API traffic** continue through the public execution surface;
- **cross-node evidence** comes from `rchain-observer`;
- raw validator HTTP ports do not need to become the browser-facing API;
- the observer itself listens on loopback and is exposed only through a read-only reverse-proxy route.

This means a DApp can submit through one execution path without being forced to use that same path as its only source of finality evidence.

---

## Live endpoints

### Public node surface

```text
https://testnet.rhobot.net
```

### Observer health

```text
GET https://testnet.rhobot.net/observer/health
```

Example shape:

```json
{
  "status": "ok",
  "observedNodes": 4,
  "reachableNodes": 4,
  "verdict": "agreement",
  "observedAtMs": 0
}
```

`observedAtMs` is a live timestamp; the zero above is only a documentation placeholder.

### Full evidence report

```text
GET https://testnet.rhobot.net/observer/v1/observation
```

A live report contains:

```text
observation timestamp
height summary
per-node reachability
network id
shard id
latest observed block
latest finalized block
finalized block hash
node-local failure, if any
cross-node verdict
reference finalized identity, when available
ancestry checks, when finalized heights differ
```

The public response intentionally exposes node labels such as `A`, `B`, `C`, and `D` without publishing the internal observation URLs used by the service.

---

# 2. What one observation cycle actually does

Every refresh interval, the observer queries each configured validator concurrently.

For every node it reads:

```text
GET /api/status
GET /api/last-finalized-block
```

From those replies it records an `Observation`:

```text
node label
reachable?
network id
shard id
latest block
finalized block
finalized hash
error, if the observation failed
```

Then it computes network-level height ranges and classifies the finalized observations. If finalized heights differ without a same-height hash conflict, the observer performs a bounded ancestry check over the higher block's node-reported justification DAG using `GET /api/block/{hash}` before refining the verdict.

The live service uses a **bounded polling loop** and serves the most recent cached snapshot to HTTP clients. A browser refresh therefore does **not** trigger a fresh fan-out of requests to every validator.

Current live-service cadence:

```text
observer poll interval   5 seconds
per-node timeout         2.5 seconds
ancestry total timeout   5 seconds per check
ancestry block budget    512 blocks per check
public API               cached snapshot
observer listener        loopback only
service memory ceiling   96 MB
```

The intent is simple: observation load is controlled by the observer process, not multiplied by page traffic.

---

# 3. Verdict semantics

The verdict is not a decorative status light. It is a compact statement about the exact evidence returned by the configured nodes.

| Verdict | Meaning |
|---|---|
| `agreement` | every comparable node reports the same finalized identity |
| `proven_lag` | finalized heights differ and every lower finalized identity was found in the justification ancestry of the highest finalized reference |
| `height_skew` | finalized heights differ but ancestry could not be completed within the available evidence/budget; uncertainty is preserved |
| `majority` | a strict majority shares one finalized identity while a conflicting same-height hash is present |
| `conflicting_finalized_history` | a complete ancestry traversal reached the lower finalized height without finding that lower finalized hash |
| `divergence` | network/shard identity conflicts, or same-height finalized hashes conflict without a strict majority |
| `unavailable` | no comparable finalized snapshot was returned |

A finalized identity is:

```text
(network id, shard id, finalized block number, finalized block hash)
```

## The important distinction: lag is not divergence

These two observations are different:

```text
A: finalized 711 / hash H711
B: finalized 710 / hash H710
```

and:

```text
A: finalized 711 / hash X
B: finalized 711 / hash Y
```

The first enters the ancestry verifier. The observer walks backward from the higher finalized block through its reported `justifications` until one of three things happens:

```text
lower finalized hash is reached
        → proven_lag

all reachable branches reach/bypass the lower height without that hash
        → conflicting_finalized_history

required block evidence is missing, malformed, times out, or exceeds the traversal budget
        → height_skew
```

The second example is already a **same-height finalized-hash conflict** and does not need ancestry inference.

`rchain-observer` keeps those cases separate because collapsing them into one red state would destroy the information an operator or application actually needs.

> A different height becomes `proven_lag` only after a concrete justification path is found. Incomplete evidence remains `height_skew`; it is never promoted by height ordering alone.

---

# 4. Why a multi-node evidence layer exists

A single RPC call can be completely valid and still be a single observation point.

```text
DApp
  │
  ▼
one HTTP endpoint
  │
  ▼
one view of the chain
```

That is often enough for development, but it creates an implicit authority boundary: the application cannot distinguish “the network says this” from “this server says the network says this.”

The observer changes the shape:

```text
                 node A ─┐
                 node D ─┤
DApp ← Observer ← node B ─┤→ explicit evidence
                 node C ─┘
```

The value is not that four HTTP responses are magically more trustworthy. The value is that **disagreement becomes observable instead of invisible**.

That matters for:

- finalized-state UX;
- explorers;
- wallets;
- asset ownership displays;
- exchange reserve/custody views;
- deploy receipts;
- operational monitoring;
- incident analysis;
- testnet regression evidence.

---

# 5. The evidence report

A report has three levels.

## A. Per-node observations

Each configured node keeps its own evidence row.

Conceptually:

```json
{
  "node": "A",
  "reachable": true,
  "network_id": "testnet",
  "shard_id": "/root",
  "latest_block": 715,
  "finalized_block": 711,
  "finalized_hash": "…",
  "error": null
}
```

The numbers above are an example shape, not a pinned promise about current chain height.

## B. Height summary

The report preserves the range visible across reachable nodes:

```text
reachable_nodes
latest_min
latest_max
finalized_min
finalized_max
```

A synchronized network may produce a single-value range. A lagging node makes the range widen without forcing the classifier to call that a hash conflict.

## C. Ancestry evidence

When finalized heights differ, each unique lower finalized identity receives an ancestry check against the highest finalized reference. The public report preserves:

```text
class
lower finalized identity
higher finalized identity
witness node label
examined block count
justification path, when proven
reason
```

The traversal is bounded. If the observer cannot complete the evidence graph safely, the check stays `unresolved_height_skew` rather than guessing.

## D. Verdict

The verdict records:

```text
class
observed_nodes
comparable_nodes
agreeing_nodes
summary
reference finalized identity
```

Applications should preserve the underlying node rows instead of reducing the report to a green/red badge. When something changes, the useful question is not only **what verdict changed**, but also **which observation changed and how**.

---

# 6. Failure behavior is part of the API

Distributed-system tooling becomes dangerous when failures disappear behind friendly defaults.

The observer therefore treats failures as evidence.

| Condition | Result |
|---|---|
| one node times out | that node becomes unreachable; the error is retained |
| all nodes fail | verdict becomes `unavailable` |
| one node is behind | ancestry is checked; result becomes `proven_lag`, `conflicting_finalized_history`, or remains `height_skew` if evidence is incomplete |
| same-height hashes differ | conflict is surfaced explicitly |
| network or shard differs | classified as divergence |
| malformed or incomplete node response | observation is not silently promoted to comparable evidence |

There is no “assume healthy because the last request worked” rule in the classifier.

The HTTP service can still serve the most recent cached snapshot while the next polling cycle is in progress. Consumers can use `observed_at_ms` to decide how fresh that evidence is for their own risk boundary.

---

# 7. Read-only by construction

`rchain-observer` is intentionally incapable of becoming the component that mutates the chain.

It does **not**:

- submit deploys;
- propose blocks;
- hold wallet keys;
- hold validator keys;
- call admin mutation routes;
- execute Rholang;
- decide application authorization;
- claim that different heights prove conflicting history;
- convert incomplete evidence into a consensus theorem.

The live service is also deployed behind an HTTP boundary that permits the public observer surface to be used as a read-only endpoint.

This division keeps execution authority and observation authority separate:

```text
EXECUTION AUTHORITY
wallet → signed deploy → RChain node → Rholang / RSpace

OBSERVATION AUTHORITY
validators → rchain-observer → evidence report → application
```

---

# 8. Live deployment model

The repository includes the deployment boundary used for the public testnet observer.

```text
deploy/rchain-observer-testnet.service
deploy/nginx-observer.conf
```

The systemd service:

- runs under a dedicated service account;
- binds only to `127.0.0.1:8088`;
- observes four validator endpoints;
- restarts on failure;
- disables privilege escalation;
- removes ambient Linux capabilities;
- protects the host filesystem and kernel surfaces;
- caps memory and task count.

The reverse proxy exposes only the observer HTTP surface:

```text
/observer/health
/observer/v1/observation
```

The validator topology behind the service remains an implementation detail rather than something the browser must connect to directly.

---

# 9. Run it yourself

## One-shot report

```bash
cargo run -- \
  --node a=http://node-a.example:40403 \
  --node b=http://node-b.example:40403 \
  --node c=http://node-c.example:40403 \
  once
```

The process emits one JSON report and exits.

## Serve cached observations

```bash
cargo run -- \
  --node a=http://node-a.example:40403 \
  --node b=http://node-b.example:40403 \
  --node c=http://node-c.example:40403 \
  serve --listen 127.0.0.1:8088 --refresh-ms 5000
```

Then:

```bash
curl http://127.0.0.1:8088/health
curl http://127.0.0.1:8088/v1/observation
```

The CLI accepts any set of compatible RChain HTTP endpoints; the public-testnet deployment happens to use four validators.

---

# 10. Reproducibility

The core path is deliberately small enough to audit.

```text
src/main.rs        CLI, polling service, HTTP surface
src/observer.rs    concurrent node observation + ancestry orchestration
src/ancestry.rs    bounded justification-DAG ancestry verifier
src/model.rs       observation/report/verdict/evidence model
src/verdict.rs     snapshot classifier + ancestry refinement
tests/verdict.rs   deterministic snapshot-verdict tests
tests/ancestry.rs  deterministic HTTP ancestry-evidence tests
```

Development checks:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

CI also builds a static Linux artifact for deployment so the small public-testnet host does not need to compile the Rust dependency graph in place.

The verdict tests cover the core distinctions that must not regress:

```text
exact agreement
height skew without false divergence
strict-majority same-height conflict
same-height split view
proven lag from an explicit justification path
conflicting finalized history after complete traversal
unresolved height skew when required block evidence is unavailable
```

---

# 11. Current engineering boundary

What exists now:

```text
✓ four-node live public-testnet observation
✓ cached read-only HTTP service
✓ per-node reachability
✓ network/shard checks
✓ latest-height range
✓ finalized-height range
✓ finalized-hash comparison
✓ exact agreement
✓ height-skew classification
✓ bounded justification-DAG ancestry verification
✓ explicit `proven_lag` classification
✓ explicit `conflicting_finalized_history` classification
✓ unresolved ancestry remains `height_skew`
✓ ancestry witness/path/examined-block evidence
✓ majority conflict classification
✓ divergence classification
✓ private topology redaction
✓ public Rholox consumption
```

What is deliberately not claimed yet:

```text
✗ independent cryptographic re-hash of raw block bytes
✗ historical evidence persistence
✗ cryptographically signed observer reports
✗ stake-weighted application policy
✗ independent observer-instance quorum
```

Those are separate engineering problems and should stay separate instead of being implied by one snapshot classifier.

---

# 12. Where this goes next

The next useful layers are about making evidence richer, not making the verdict louder.

1. **Raw-block cryptographic verification** — reproduce content-addressed block hashes from the node's raw encoded bytes instead of relying only on the structured block response.
2. **Bounded observation history** — preserve transitions across time instead of exposing only the latest snapshot.
3. **Stable evidence identities** — make reports addressable and reproducible across clients.
4. **Deploy-to-finality evidence** — connect a deploy id to processing, inclusion, finalized visibility, and cross-node confirmation.
5. **Configurable policy** — allow consumers to define what evidence is sufficient for their own application boundary without changing the raw observations.
6. **Independent observer instances** — separate node diversity from observer-process diversity when the deployment warrants it.

The principle is constant:

> **Never erase uncertainty just to make the interface look decisive.**

---

# 13. Rholox consumes this live evidence

[Rholox](https://rholox.com) now consumes the public observer endpoint in its Network Console.

Its execution and evidence paths are intentionally different:

```text
                         ┌─ signed writes ─────────────► public RChain node
                         │                              │
Rholox / DApp ───────────┤                              ▼
                         │                         Rholang / RSpace
                         │
                         └─ evidence reads ────────────► rchain-observer
                                                        │
                                             ┌──────────┼──────────┐
                                             ▼          ▼          ▼
                                           nodes      nodes      nodes
                                                        │
                                                        ▼
                                                explicit verdict
```

That allows the UI to say things such as:

```text
SUBMITTED
   ↓
PROCESSED
   ↓
INCLUDED
   ↓
FINALIZED
   ↓
4/4 FINALIZED SNAPSHOT AGREEMENT
```

without pretending that submission and finality are the same event.

---

# 14. Build DApps on RChain / Rholox

Only after the evidence boundary is clear does the application layer become interesting.

If Ethereum asks you to learn **Solidity + EVM semantics**, and TON asks you to learn **Tact/FunC + TVM/message semantics**, the corresponding execution language here is **Rholang**.

| Ecosystem | Contract language | Execution model | State / interaction model | Typical client layer |
|---|---|---|---|---|
| Ethereum | Solidity | EVM | storage + calls/transactions | JS/TS + JSON-RPC |
| TON | Tact / FunC | TVM | cells + asynchronous messages | JS/TS + TON APIs |
| **RChain / Rholox** | **Rholang** | **RSpace / rho-calculus** | **names, channels, sends, receives, processes** | **JS/TS + RChain HTTP APIs + Observer** |

Rholox does not replace Rholang. It is the developer/application surface around it: wallet, signing, deploy flow, proof, explorer, REV transfer, assets, exchange primitives, finality UX, and observer evidence.

## What a DApp developer should know

### Rholang

At minimum:

```text
new
send with !
receive with for
parallel composition with |
contracts
pattern matching
quoted names
unforgeable names
system processes / URNs
capability-oriented authority
```

Minimal exploratory term:

```rholang
new return in {
  return!("hello from Rholang")
}
```

The example is included at:

```text
examples/hello.rho
```

### RSpace

The useful mental model is not “which storage slot do I modify?” but:

```text
which channel owns this state?
which message is waiting?
which continuation may consume it?
which capability gives the caller authority?
```

That shift matters because Rholang contracts are built around concurrent process interaction rather than an object-with-storage abstraction.

### TypeScript / JavaScript

For a web DApp, the client layer typically owns:

```text
wallet / key boundary
deploy construction
serialization
Blake2b hashing
secp256k1 signing
HTTP submission
status polling
result decoding
finalized read-back
Observer integration
```

React is optional. TypeScript is not a consensus requirement. The important boundary is that on-chain state transitions execute as Rholang on RChain.

---

## The DApp transaction lifecycle

A serious application should preserve the whole lifecycle:

```text
Rholang term
    ↓
construct deploy
    ↓
sign locally
    ↓
SUBMITTED
    ↓
PROCESSING
    ↓
INCLUDED
    ↓
state read-back
    ↓
FINALIZED
    ↓
CROSS-NODE EVIDENCE
```

Do not collapse those into one “success” state.

The write and evidence paths should remain separate:

```text
WRITE
wallet → signed deploy → RChain → Rholang → RSpace

EVIDENCE
validators → rchain-observer → verdict → DApp
```

That is the core development model Rholox is moving toward: make the correct path easier without hiding what the network actually proved.

---

## Practical stack

```text
On-chain logic       Rholang
State substrate      RSpace
Execution            RChain node
Client               TypeScript / JavaScript
Signing              secp256k1
Deploy digest        Blake2b-256
Reads                 RChain HTTP API / explore-deploy
Finality evidence     rchain-observer
Developer surface     Rholox
```

Rust is **not** required to build a normal Rholang DApp. It matters if you are working on the observer, node infrastructure, or lower-level tooling.

For the full engineering walkthrough, including deploy construction, system processes, read paths, security rules, asset/application patterns, and recommended learning order, continue here:

## **[Building DApps on Rholox / RChain →](docs/BUILDING-DAPPS.md)**

---

## One sentence

**Execute state in Rholang, drive it from a client you control, and never let one endpoint become invisible authority over the finality you show to users.**
