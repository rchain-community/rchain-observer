# Building DApps on Rholox / RChain

This guide explains the actual engineering boundary for building applications around Rholox and RChain.

The shortest useful answer is:

> Learn **Rholang** for on-chain behavior, **TypeScript/JavaScript** for the client, and the **RChain deploy/finality model** for safely connecting the two.

`rchain-observer` then supplies the read-only multi-node evidence layer that a serious application can use instead of treating one HTTP endpoint as truth.

---

## 1. What Rholox is — and what it is not

Rholox is a developer/application surface over RChain public-testnet capabilities. It is not a new virtual machine and not a replacement contract language.

```text
RChain          = L1 execution + consensus + finalized state
Rholang         = contract/process language
RSpace          = state / communication substrate
Rholox          = wallet + deploy + proof + explorer + asset/application surface
rchain-observer = multi-node finalized-state evidence
Your DApp       = domain logic + product UX on top
```

The distinction matters because a Rholox DApp still executes on RChain.

---

## 2. What should I learn?

### Required: Rholang

If you are writing state transitions, permissions, contract behavior, asset logic, or another on-chain protocol, Rholang is the core language.

You should understand:

- fresh names with `new`;
- sends with `!`;
- receives with `for`;
- process composition with `|`;
- contracts / persistent behavior;
- pattern matching;
- quoting and names;
- private/unforgeable capabilities;
- public URNs and installed system processes;
- concurrency and reduction;
- how authority can be represented by possession of a name/capability.

### Required for web DApps: TypeScript or JavaScript

Use it for:

- wallet/key management;
- address derivation;
- deploy construction;
- signing;
- HTTP calls;
- response decoding;
- inclusion/finality state machines;
- UI/product state;
- observer integration.

Rholox itself is currently a React/TypeScript application, but React is not a chain requirement. A CLI, native app, service, game client, or another frontend can use the same underlying model.

### Strongly recommended: cryptographic basics

At minimum know what these mean:

- secp256k1 private/public keys;
- signatures;
- Blake2b / Keccak hashing;
- address derivation;
- local signing vs key custody;
- why a private key should never be uploaded merely to submit a deploy.

### Strongly recommended: distributed-systems finality

Know the difference between:

- request accepted;
- deploy pooled;
- deploy processed;
- block inclusion;
- finalized state;
- one-node observation;
- cross-node agreement.

This is not cosmetic terminology. It determines what your UI is allowed to claim.

### Optional: Rust

Rust is **not** required to write a Rholang DApp. It matters if you are contributing to `rchain-observer`, node infrastructure, backend services, or lower-level tooling.

---

## 3. The Rholang mental model

A useful first approximation is a concurrent tuple-space machine.

In a storage-oriented smart-contract model you may ask:

```text
which field do I update?
```

In Rholang you often ask:

```text
which channel owns this state?
which message is waiting?
which continuation may consume it?
which name gives the caller authority?
```

That difference affects architecture.

### Minimal term

`examples/hello.rho` contains:

```rholang
new return in {
  return!("hello from Rholang")
}
```

The first private name can act as the result channel for exploratory execution in the current node API convention.

### Core shape

Conceptually:

```rholang
new state, request in {
  /* state process */
  |
  /* request handling process */
}
```

Real contracts add persistent behavior, pattern matching, authority checks, and replies. Keep the security question explicit: **what capability must a caller possess to cause this state transition?**

---

## 4. Rholang system facilities you will encounter

A DApp can call into installed/public system processes. Depending on the contract, important facilities include:

```text
rho:registry:lookup
rho:registry:insertArbitrary
rho:registry:insertSigned:secp256k1
rho:crypto:sha256Hash
rho:crypto:keccak256Hash
rho:crypto:blake2b256Hash
rho:crypto:secp256k1Verify
rho:crypto:ed25519Verify
rho:rev:address
rho:rchain:deployerId:ops
rho:rchain:revVault
rho:rchain:makeMint
```

The exact contract API matters. Do not guess reply shapes from names; pin them to the network/node API you are targeting.

### Deploy identity is contextual

On a real deploy path, the runtime binds deploy-specific identity channels such as deploy id / deployer id.

On exploratory execution, those deploy-bound values are absent. Therefore this pattern is unsafe:

```text
"explore passed, therefore the real deploy path is identical"
```

Use exploratory execution for what it can prove: parsing, deterministic branches that do not depend on deploy-only context, and read-oriented terms. Test deploy-only authorization on the deploy path.

---

## 5. The client write path

A signed write generally has this shape:

```text
Rholang source
    │
    ▼
DeployData
    ├── term
    ├── timestamp
    ├── phloPrice
    ├── phloLimit
    ├── validAfterBlockNumber
    └── shardId
    │
    ▼
serialization
    │
    ▼
Blake2b-256 digest
    │
    ▼
secp256k1 signature
    │
    ▼
POST /api/deploy
```

A client should fetch live node context instead of hard-coding chain values such as shard or current height.

### Current Rholox behavior

The current client implementation:

1. reads `/api/status`;
2. constructs deploy data;
3. serializes the deploy envelope;
4. computes a Blake2b digest;
5. signs locally with secp256k1;
6. posts the signed deploy to `/api/deploy`;
7. extracts the returned deploy id;
8. polls `/api/v1/deploy-status/{deployId}`.

Private keys are a client concern. A DApp should not move a key to a server simply because submitting HTTP requests from a server is easier.

---

## 6. The read path

RChain exposes different read surfaces for different questions.

### Node status

```text
GET /api/status
```

Use it for network/shard/current-node context.

### Exploratory Rholang

```text
POST /api/explore-deploy
```

The request body is the Rholang term as a raw JSON string on the current API contract.

Use it for read-oriented computation and preflight paths where deploy-only context is not required.

### Deploy status

```text
GET /api/v1/deploy-status/{deployId}
```

A deploy status may represent success, execution error, or not-yet-processed state. Your decoder must preserve those distinctions.

### Finalized block

```text
GET /api/last-finalized-block
```

This is the core surface currently used by `rchain-observer` alongside `/api/status`.

---

## 7. Do not collapse the lifecycle

A robust application state machine looks like this:

```text
DRAFT
  ↓
SIGNED
  ↓
SUBMITTED
  ↓
PROCESSING
  ↓
INCLUDED
  ↓
STATE READ-BACK
  ↓
FINALIZED
  ↓
CROSS-NODE VERIFIED
```

Not every application needs every UI label, but the implementation should preserve the semantics.

### Why this matters

A deploy can be accepted without being final.

A block can be observed without being the finalized block.

A state read from one gateway can be valid as an observation without being independently cross-checked.

Rholox is intentionally moving toward evidence-first UX rather than presenting each intermediate step as irreversible success.

---

## 8. Where `rchain-observer` enters the DApp

The observer sits on the read/evidence side:

```text
                  ┌─────────────── node A
                  ├─────────────── node D
DApp ─► Observer ─┼─────────────── node B
                  └─────────────── node C
                         │
                         ▼
                finalized snapshots
                         │
                         ▼
             explicit comparison verdict
```

It currently requests, per node:

```text
GET /api/status
GET /api/last-finalized-block
```

and records:

```text
node name
base URL
reachable
network id
shard id
latest block
finalized block
finalized hash
error
```

The classifier first compares finalized identities. When heights differ without a same-height hash conflict, the observer then verifies ancestry by walking the higher finalized block's node-reported justification DAG through `GET /api/block/{hash}`.

### Current verdicts

```text
agreement
proven_lag
height_skew
majority
conflicting_finalized_history
divergence
unavailable
```

### Correct application behavior

Do not map every non-agreement to “chain failure.”

A different finalized height is only promoted to `proven_lag` when the lower finalized hash is actually reachable from the higher finalized block's justification ancestry. If a complete traversal excludes that hash, the observer reports `conflicting_finalized_history`. Missing, malformed, timed-out, or budget-exhausted ancestry evidence remains `height_skew`.

---

## 9. A practical DApp architecture

For a browser DApp:

```text
src/contracts/*.rho
      │
      │ Rholang
      ▼
contract behavior

src/chain/*.ts
      │
      ├── build terms
      ├── sign deploys
      ├── submit
      ├── poll status
      ├── decode results
      └── finalized read-back

src/evidence/*.ts
      │
      └── rchain-observer client

src/ui/*
      │
      └── render state without collapsing lifecycle semantics
```

A backend can use the same model. The rule is not “everything must be in the browser.” The rule is that **signing authority, mutation authority, and observation authority should not be silently conflated**.

---

## 10. Example end-to-end flow

Assume a DApp has a Rholang operation that changes state.

### Step A — build the term

Keep contract/source generation deterministic. Treat user input as data, not source code, whenever possible.

### Step B — preflight safe read paths

Use `explore-deploy` for queries or branches that do not depend on deploy-only channels.

### Step C — construct the deploy

Read live node status and set:

```text
phlo price
phlo limit
valid-after block
shard id
```

### Step D — sign locally

The key signs the deploy digest. The private key stays in the wallet/client boundary.

### Step E — submit

```text
POST /api/deploy
```

Capture the deploy id. This means submission was accepted, not finality.

### Step F — poll processing

```text
GET /api/v1/deploy-status/{deployId}
```

Handle processing success and processing error explicitly.

### Step G — capture inclusion context

Record the block associated with processed success.

### Step H — read state back

Ask the chain for the state that proves the application-level effect you care about: balance, object, registry value, asset ownership, pool reserves, etc.

### Step I — wait for finality

Do not label the action “finalized” merely because a block contains it.

### Step J — cross-check

Call the observer and verify that configured endpoints report a compatible finalized view before making a high-confidence finality claim.

---

## 11. Assets and application protocols

Rholox already demonstrates that Rholang can back higher-level application primitives rather than only isolated toy contracts.

The public-testnet surface includes work around:

- REV transfers;
- proof anchoring;
- fixed-supply MakeMint assets;
- capability-based purse handling;
- pool custody;
- deterministic integer AMM arithmetic;
- add/remove liquidity;
- swaps;
- compensation/fail-closed recovery paths;
- finalized custody/reserve checks.

These are examples of application infrastructure, not a claim that every primitive is production-ready or audited.

The useful pattern for a new DApp is:

```text
Rholang authority/state machine
        +
client-side signing/integration
        +
finalized read-back
        +
observer evidence
```

---

## 12. Security rules for DApp authors

### Keep keys local to the signer boundary

A browser-generated key, hardware key, mobile wallet, or service signer can all be valid architectures. What matters is that an unrelated API server should not receive the key just to relay a deploy.

### Model authority explicitly

If possession of a name grants an operation, treat the name as a capability. Do not serialize or expose it casually.

### Fail closed on state mismatch

If recorded application state disagrees with authoritative on-chain custody/state, stop mutation until the mismatch is understood.

### Distinguish read trust from write trust

The node that accepted a deploy does not have to be the only node you consult for finality.

### Preserve errors

Do not turn malformed response, timeout, not-processed, execution failure, and divergence into one generic “failed” state. Those conditions have different operational meanings.

### Never invent telemetry

If you do not have a measured value for TVL, TPS, APR, liquidity, price, peer count, or validator state, do not synthesize one for the UI.

---

## 13. What `rchain-observer` proves — and what it still does not

For different finalized heights, the observer now checks whether the lower finalized hash is reachable from the higher finalized block through the node-reported justification DAG. That is enough to distinguish:

```text
proven_lag
conflicting_finalized_history
unresolved height_skew
```

The traversal is bounded and fail-closed: incomplete block evidence never becomes a positive ancestry claim.

The current layer still does **not** independently reconstruct the block hash from raw encoded block bytes, persist a historical evidence ledger, or establish agreement between multiple independent observer processes. Those are separate evidence-strengthening layers rather than prerequisites for ordinary ancestry classification.

---

## 14. Recommended learning order

For an Ethereum developer:

```text
1. Rholang process/channel syntax
2. RSpace state model
3. installed system processes
4. deploy signing and node API
5. finality lifecycle
6. observer/evidence integration
```

For a frontend developer:

```text
1. TypeScript client integration
2. wallet/signing boundary
3. node status + explore reads
4. deploy status state machine
5. enough Rholang to understand contract interfaces
6. finalized read-back + observer
```

For a protocol developer:

```text
1. Rholang deeply
2. RSpace / concurrency / capabilities
3. system contracts
4. finality/consensus semantics
5. deterministic client contracts
6. evidence and negative controls
```

---

## 15. Minimal local stack

For observer development:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

For a Rholox-style web client, a practical stack is:

```text
TypeScript
React or another UI framework
secp256k1 implementation
Blake2b
HTTP/JSON client
Rholang source/templates
observer client
```

None of those frontend choices are consensus requirements. Rholang + the chain API are the execution boundary.

---

## 16. Target developer experience

The end state should feel less like “learn every internal node detail before you can build anything” and more like:

```text
1. write Rholang
2. import a Rholox client package
3. connect a signer
4. submit
5. await processed
6. await finalized
7. verify with Observer
8. ship the DApp
```

The important part is that convenience must not erase the evidence model underneath it.

Rholox should make the correct path shorter, not make the chain look simpler than it is.

---

## References inside this codebase

- [`../src/observer.rs`](../src/observer.rs) — concurrent node reads
- [`../src/verdict.rs`](../src/verdict.rs) — verdict classifier
- [`../src/main.rs`](../src/main.rs) — CLI and HTTP API
- [`../examples/hello.rho`](../examples/hello.rho) — smallest Rholang example
- [Rholox](https://rholox.com) — public-testnet application surface
