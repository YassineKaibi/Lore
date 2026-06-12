# Lore

**An intent graph over your codebase — what the code does, why it does it, and whether the two still agree.**

A developer has never been able to ask their system a question and trust the answer. Source code is the truth but illegible at scale; artifacts *about* code (comments, docs, wikis) are legible but untrustworthy — they rot silently. Lore ends that trade-off by maintaining one intent graph with two layers and checking them against each other:

- **Derived layer — the *what*.** Facts extracted from source by static analysis: functions, calls, state reads and writes. True by construction, available with zero annotations.
- **Declared layer — the *why*.** `@lore` annotation blocks in ordinary comments carrying knowledge no analysis can derive: `purpose`, `because`, `assumes`, `unknown`, `owner`, and effect claims like `affects` and `reads`.
- **Reconciliation — the honesty mechanism.** Every declared claim is checked against the derived layer and labeled **Verified**, **Unverified**, **Contradicted**, or **Unverifiable**. Drift between docs and code becomes a CI finding, not a silent decay.

The result is a graph you can query (`lore ask`), lint in CI (`lore lint`), and trust — because every answer is labeled with how it was established. Lore never presents a guess as a fact: derived edges carry an explicit confidence (`Exact`, `Resolved`, `Heuristic`), unresolvable calls are counted rather than guessed, and `Unverifiable` is an honest answer.

## Example

An `@lore` block lives in a regular comment directly above the declaration it describes:

```python
# @lore
# kind: state
# purpose: "Append-only record of every balance-changing event"
# owner: "payments-team"
# because: "Auditors require a reconstructible balance history (TICKET-841)"
class Ledger:
    ...
```

```python
# @lore
# purpose: "Apply a refund and record it"
# affects: Payment.Ledger
def refund(payment_id: str, amount: Decimal) -> None:
    ...
```

The scanner finds the blocks, the binder attaches each one to its subject via tree-sitter, the derived layer extracts what `refund` actually touches, and reconciliation checks the `affects: Payment.Ledger` claim against reality. If someone later rewrites `refund` to stop writing to the ledger, the claim flips to `Contradicted` and `lore lint` fails the build.

Then you can ask:

```sh
lore ask 'affects*(Payment.Ledger)'   # everything that transitively writes the ledger
lore ask 'show(Payment.Ledger)'       # one node: intent, edges, claim statuses
lore ask 'unknown'                    # every declared open question in the project
```

## Installation

Lore is not yet published to crates.io. Build from source (requires a Rust toolchain with edition 2024 support):

```sh
git clone <repository-url>
cd Lore
cargo install --path crates/lore_cli
```

## Quick start

```sh
cd your-project
lore init     # detect languages, write a starter lore.toml
lore scan     # list every @lore block, its subject, qname, kind
lore lint     # structural + reconciliation findings; exit 1 on errors
lore stats    # coverage: nodes by kind/origin, declared intent, unresolved calls
```

Annotations are recognized in **Python, TypeScript, and Rust** comments. The derived layer (automatic extraction, no annotations needed) currently covers **Python and TypeScript**; more languages are on the roadmap.

## Commands

| Command | What it does |
|---|---|
| `lore init` | Write a starter `lore.toml`: detect languages, propose `[modules]` globs |
| `lore scan` | Scanner + binder only: every annotation block, its subject, qname, kind |
| `lore lint` | Resolution, required intent, applicability, hygiene, and claim reconciliation; exit 1 on error-severity findings |
| `lore ask '<query>'` | Answer a query over the intent graph (see below) |
| `lore history <qname>` | Render the git change history of a node's subject span |
| `lore stats` | Coverage counts: nodes by kind/origin, declared intent per kind, unresolved calls |

Every command takes `--json` for machine-readable output. Exit codes are stable: `0` success, `1` error-severity findings, `2` usage or input parse error, `3` internal error.

## Query language

`lore ask` answers a small, closed set of query forms; a trailing `*` makes a query transitive:

```text
affects(X)  reads(X)  touches(X)  triggers(X)     — effect queries
emits(X)  handlers(X)                              — event queries
depends(X)  dependents(X)  reaches(X)  path(X, Y)  — dependency queries
show(X)  tagged("...")  owner("...")  unknown      — inspection queries
```

Results can be narrowed with filters: `in module(X)`, `in service(X)`, `owned_by("team")`, `kind(state)`. For example:

```sh
lore ask 'dependents*(Billing.Invoice) kind(workflow)'
lore ask 'unknown in module(Payments)'
```

Output is deterministic — results are deduplicated, sorted, and every edge in an answer carries its origin and status, so you always know whether you are reading a verified fact or an unreconciled claim.

## The manifest: `lore.toml`

A project is rooted by a `lore.toml`:

```toml
[project]
name = "my-project"
languages = ["python", "typescript"]
roots = ["src"]

[modules]
"src/payments/**" = "Payments"
"src/billing/**"  = "Billing"
```

`[modules]` maps file globs to module nodes in the graph. Order is normative: the first matching glob wins.

## CI integration

`lore lint` is designed to gate merges: it exits `1` when any error-severity finding exists (including `Contradicted` claims), and findings are deterministically ordered so diffs are stable. See [`examples/ci-sample`](examples/ci-sample) for a minimal workflow that goes red on a seeded violation. This repository dogfoods Lore on itself — `cargo run -p lore_cli -- lint` runs in CI against Lore's own `@lore` annotations.

## Architecture

Lore is a Rust workspace of five crates with a strict dependency order:

```
lore_intent → lore_annotations → lore_derive → lore_graph → lore_cli
```

| Crate | Role |
|---|---|
| `lore_intent` | The shared contract: `Intent`, `IntentNode`, `Edge`, `Graph` types and the clause parser. Every other crate consumes these types. |
| `lore_annotations` | Scanner (find `@lore` blocks in comments) and tree-sitter binder (attach each block to its subject declaration). |
| `lore_derive` | The derived layer: static extraction of nodes and confidence-labeled edges from host source. |
| `lore_graph` | Graph construction: node table, resolution, structural and derived edges, reconciliation, hygiene checks, and the query engine. Consumes *data* from upstream crates, never the crates themselves. |
| `lore_cli` | `clap` wiring, manifest discovery, output shaping (human and `--json`), exit codes. |

## Project status

Lore is pre-1.0 (workspace version 0.2.0) and under active development. Phase 1 — the language-agnostic annotation tool — is well underway:

- ✅ Scanner, binder, and clause parser (Python, TypeScript, Rust)
- ✅ Graph construction, structural lint, and the query engine
- ✅ Ownership and history integration (`lore history`, CODEOWNERS cross-check)
- ✅ Derived layer for Python and TypeScript
- ✅ Reconciliation: the full four-status claim labeling, plus staleness detection
- 🚧 Next: derivation for more languages (Go, Java, Rust) and graph export

Phase 2 — a dedicated `.lore` language in which every effect declaration is checked by the compiler — is specified but explicitly gated behind Phase 1's exit criteria.

## Development

```sh
cargo test --workspace                              # full test suite
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p lore_cli -- lint                       # dogfood: lint Lore with Lore
```

The project is documentation-driven, and the documents are binding:

- [`docs/lore-spec.md`](docs/lore-spec.md) — the authoritative specification. Code that contradicts the spec is a bug in the code.
- [`docs/lore-decisions.md`](docs/lore-decisions.md) — append-only decisions ledger (`D-NNN`). Spec gaps are resolved by ledger entry and spec update *before* code.
- [`docs/lore-guidelines.md`](docs/lore-guidelines.md) — engineering rules (`G-1`..`G-14`) that reviews cite by number.
- [`docs/lore-roadmap.md`](docs/lore-roadmap.md) — milestones with binding exit criteria; order is mandatory.

Contributions should start from the spec and ledger; commit messages explain *why* and cite spec sections (`§N.N`), decisions (`D-NNN`), and guidelines (`G-N`).
