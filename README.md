# Veridikt

**Your comments claim what the code does. Veridikt checks whether they're still telling the truth - and fails CI when they aren't.**

---

## The problem

Every codebase carries two kinds of knowledge: what the code *does* (true, but illegible at scale) and what humans wrote *about* it - comments, docs, wikis (legible, but untrustworthy, because nothing stops them rotting the moment the code moves on).

## What Veridikt does differently

Veridikt maintains **one intent graph with two layers** and checks them against each other:

- **Derived layer - the *what*.** Functions, calls, and state reads/writes extracted from source by static analysis. True by construction, available with **zero annotations**.
- **Declared layer - the *why*.** `@veridikt` blocks in ordinary comments carrying what no analysis can recover: `purpose`, `because`, `assumes`, `unknown`, `owner`, and effect claims like `affects` and `reads`.
- **Reconciliation - the honesty mechanism.** Every declared claim is labeled against the derived layer: **Verified**, **Unverified**, **Contradicted**, or **Unverifiable**. Drift becomes a CI finding, not silent decay.

Veridikt never presents a guess as a fact. Derived edges carry an explicit confidence (`Exact`, `Resolved`, `Heuristic`), unresolvable calls are *counted, never invented*, and `Unverifiable` is a first-class, honest answer. Annotations and derivation work for **Python, TypeScript, Rust, Go, and Java**.

## Before / after

**Before** - a comment that was true once. Nothing connects it to reality; the day someone stops writing to the ledger, it lies silently:

```python
# Applies a refund and records it in the ledger.
def refund(payment_id: str, amount: Decimal) -> None:
    ...
```

**After** - the same intent, as a claim Veridikt can check:

```python
# @veridikt
# purpose: "Apply a refund and record it"
# affects: Payment.ledger
def refund(payment_id: str, amount: Decimal) -> None:
    ledger.append(LedgerEntry(payment_id, -amount))
```

Now `veridikt lint` resolves `Payment.ledger`, derives what `refund` *actually* writes, and reconciles the claim:

- While `refund` writes the ledger → `affects: Payment.ledger` is **Verified**.
- Delete the `ledger.append(...)` line but leave the comment → the claim flips to **Contradicted** (`W0302`/`E0302`) and **lint fails the build**.
- Change the body without touching the block → **stale-intent** (`W0301`), pointing at the commit that moved on.

The comment and the code now police each other. That symmetry is the entire point.

And because the derived layer stands alone, you can ask questions no comment ever answered:

```sh
veridikt ask 'affects*(Payment.ledger)'   # everything that transitively writes the ledger
veridikt ask 'show(Payment.ledger)'       # one node: intent, edges, every claim's status
veridikt ask 'unknown'                    # every declared open question in the project
```

Every line of output carries its origin and trust - so you always know whether you're reading a verified fact or an unreconciled claim.

## 5-minute quickstart

Veridikt is pre-1.0 and not yet on crates.io - build from source (needs a Rust toolchain with edition 2024):

```sh
git clone https://github.com/YassineKaibi/Veridikt && cd Veridikt
cargo install --path crates/veridikt_cli
```

Then, in your own project:

```sh
cd your-project
veridikt init      # detect languages, write a starter veridikt.toml
veridikt scan      # list every @veridikt block: subject, qname, kind
veridikt lint      # structural + reconciliation findings; exit 1 on errors
veridikt stats     # coverage: nodes by kind/origin, declared intent, unresolved calls
```

`veridikt init` writes a `veridikt.toml` that roots the project and maps file globs to module names (first matching glob wins):

```toml
[project]
name = "my-project"
languages = ["python", "typescript"]
roots = ["src"]

[modules]
"src/payments/**" = "Payment"
"src/billing/**"  = "Billing"
```

Add your first `@veridikt` block above a declaration, rerun `veridikt lint`, and watch the claim get labeled. That's the loop.

### Wire it into CI

`veridikt lint` is built to gate merges: it exits `1` on any error-severity finding (including `Contradicted` claims), with deterministically-ordered output so diffs are stable. See [`examples/ci-sample`](examples/ci-sample) for a workflow that goes red on a seeded violation. This repo dogfoods Veridikt on itself - `cargo run -p veridikt_cli -- lint` runs in its own CI.

---

## Query reference

`veridikt ask` answers a small, closed set of forms; a trailing `*` makes a query transitive:

```text
affects(X)  reads(X)  touches(X)  triggers(X)     - effect queries
emits(X)  handlers(X)                              - event queries
depends(X)  dependents(X)  reaches(X)  path(X, Y)  - dependency queries
show(X)  tagged("...")  owner("...")  unknown      - inspection queries
```

Narrow any result with filters: `in module(X)`, `in service(X)`, `owned_by("team")`, `kind(state)`. Output is deduplicated, sorted, and stable.

## Visualize the graph

`veridikt graph --dot` emits [Graphviz](https://graphviz.org/) DOT - pipe it into `dot`:

```sh
# A focused, legible neighborhood around one node (what you usually want)
veridikt graph --dot --focus Payment.ledger --depth 1 | dot -Tpng > ledger.png

# Whole project → a zoomable SVG
veridikt graph --dot | dot -Tsvg > graph.svg
```

Every edge label carries its trust (`Calls (Exact)`, `Affects (Heuristic)`), so a rendered graph never launders a guess into a fact.

## Commands

| Command | What it does |
|---|---|
| `veridikt init` | Write a starter `veridikt.toml`: detect languages, propose `[modules]` globs |
| `veridikt scan` | Scanner + binder only: every block, its subject, qname, kind |
| `veridikt lint` | Resolution, required intent, applicability, hygiene, reconciliation; exit 1 on errors |
| `veridikt ask '<query>'` | Answer a query over the intent graph |
| `veridikt graph --dot [--focus <qname> --depth N]` | Export the graph as Graphviz DOT |
| `veridikt history <qname>` | Render the git change history of a node's subject span |
| `veridikt stats` | Coverage: nodes by kind/origin, declared intent per kind, unresolved calls |

Every command takes `--json`. Exit codes are stable: `0` success, `1` error-severity findings, `2` usage error, `3` internal error.

---

## Project status

Pre-1.0 (workspace `0.2.0`), under active development. **Phase 1 - the language-agnostic annotation tool - is feature-complete:**

- Scanner, binder, clause parser
- Graph construction, structural lint, query engine
- Ownership + history integration (`veridikt history`, CODEOWNERS cross-check)
- Derived layer for Python, TypeScript, Rust, Go, and Java
- Reconciliation: full four-status claim labeling + staleness detection
- Language packs: declarative, conformance-tested, externally loadable
- Graph export (`veridikt graph --dot`)

**Phase 2** - a dedicated `.veridikt` language in which the compiler checks *every* effect declaration against actual call sites - is specified but explicitly gated behind Phase 1's exit criteria.

## Contributing

Language support lives in declarative **packs** (`packs/<lang>/`) - a manifest plus tree-sitter queries, no per-language Rust for most languages. Adding a language is mostly data. See **[CONTRIBUTING.md](CONTRIBUTING.md)** for the architecture summary, the pack workflow, and what makes a good first issue.

The project is documentation-driven, and the documents are binding:

- [`docs/veridikt-spec.md`](docs/veridikt-spec.md) - the authoritative specification.
- [`docs/veridikt-decisions.md`](docs/veridikt-decisions.md) - append-only decisions ledger (`D-NNN`).
- [`docs/veridikt-guidelines.md`](docs/veridikt-guidelines.md) - engineering rules (`G-1`..`G-14`).
- [`docs/veridikt-roadmap.md`](docs/veridikt-roadmap.md) - milestones with binding exit criteria.

## Development

```sh
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p veridikt_cli -- lint   # dogfood: lint Veridikt with Veridikt
```
