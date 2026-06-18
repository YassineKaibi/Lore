# Contributing to Veridikt

Thanks for considering a contribution. Veridikt is documentation-driven: the spec and ledger are binding, and the fastest way to get a PR merged is to ground it in them.

## Architecture in 200 words

Veridikt builds **one intent graph with two layers** over a codebase. The *derived* layer is extracted from source by static analysis (true by construction); the *declared* layer comes from `@veridikt` blocks in comments (human intent: `purpose`, `because`, `affects`, …). A reconciliation pass labels every declared claim `Verified`, `Unverified`, `Contradicted`, or `Unverifiable`.

It's a Rust workspace (edition 2024) of five crates in a strict dependency order - never start a crate before the previous one's core works:

```
veridikt_intent → veridikt_annotations → veridikt_derive → veridikt_graph → veridikt_cli
```

- **`veridikt_intent`** - shared types (`Intent`, `IntentNode`, `Edge`, `Graph`) and the clause parser. Everything consumes these.
- **`veridikt_annotations`** - scanner (find `@veridikt` blocks) + tree-sitter binder (attach each to its subject).
- **`veridikt_derive`** - the derived layer: nodes, confidence-labeled edges, the pack loader and import strategies.
- **`veridikt_graph`** - graph construction, resolution, reconciliation, hygiene checks, query engine. Consumes upstream *data*, never the crates.
- **`veridikt_cli`** - `clap` wiring, manifest discovery, output shaping, exit codes.

Language support lives in declarative **packs** (`packs/<lang>/`): a manifest plus tree-sitter queries, no per-language Rust for most languages.

## Read these first

- [`docs/veridikt-spec.md`](docs/veridikt-spec.md) - the source of truth. Code that contradicts it is a bug in the code.
- [`docs/veridikt-decisions.md`](docs/veridikt-decisions.md) - the append-only `D-NNN` ledger. A spec gap is resolved by a new D-entry + spec update *before* code.
- [`docs/veridikt-guidelines.md`](docs/veridikt-guidelines.md) - rules `G-1`..`G-14`. Reviews cite these by number.
- [`docs/veridikt-roadmap.md`](docs/veridikt-roadmap.md) - milestones; their order is mandatory.

Two guidelines bite on almost every PR: **G-7** (never present a guess as a fact - drop and count instead of inventing an edge) and **G-11** (unhappy path first - implement and test the failure case before the success case).

## What makes a good first issue

Issues labeled **`good first issue`** meet all of these:

- **Scoped to one crate or one pack**, with the relevant spec section named in the issue.
- **Has a clear input → exact output**, so it can be tested at the crate boundary (G-4) without inventing new public surface.
- **Doesn't require a spec or ledger change.** Anything that does is labeled `needs-decision` and we'll work the D-entry out with you first.
- **No tree-sitter grammar bumps** (those are pinned deliberately; see roadmap risk 4).

Good first contributions in practice:

- A new **conformance fixture** for an existing pack - especially a negative case (a call that must drop, a block that must fail to bind).
- A **lower-tier language pack** (`scan` or `bind`) for a language we don't ship yet - mostly data plus `.scm` queries.
- A **diagnostic message improvement**: clearer prose for an existing `E`/`W` code (assert the code, not the prose - G-5).
- **Docs**: a spec example that doesn't yet appear in the README, or a clarifying note.

If you're unsure whether something fits, open an issue describing the input and the output you expect before writing code.

## PR workflow

1. **Open an issue first** for anything non-trivial, so we can confirm scope and the right crate (G-14) before you invest time.
2. **Branch** from `main` - one logical change per branch.
3. **Write the test at the boundary first** (G-4), unhappy path before happy path (G-11). New diagnostics get a code from spec §18 added in the same PR (G-5).
4. **Green locally** before pushing:
   ```sh
   cargo test --workspace
   cargo fmt --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo run -p veridikt_cli -- lint        # Veridikt lints itself
   ```
   New pack work also needs: `cargo test -p veridikt_cli --test conformance`.
5. **Commit messages explain *why*** and cite spec/ledger/guidelines (`§N.N`, `D-NNN`, `G-N`) - `veridikt history` renders them back to users (G-10).
6. **Open the PR** against `main`. Describe the behavior change and link the issue. CI runs the four checks above; all must be green.

## Response times I'm committing to

This is currently a solo-maintained project, so these are honest targets, not an SLA:

- **New issues and PRs: an initial response within 3 business days.** Even if it's just "looks good, reviewing properly this weekend."
- **`good first issue` PRs: a full review within 1 week.**
- **Stale-PR courtesy:** if I go quiet for more than a week on an open PR, ping the thread - that's a bug on my side, not a hint.

If a contribution stalls on a design question, I'll convert the discussion into a `D-NNN` ledger entry so the reasoning is recorded and the next person doesn't re-litigate it.
