# Reconciliation and Staleness

## Description

Reconciliation is Veridikt's honesty mechanism: the pass that checks every declared effect claim against the derived layer and labels it `Verified`, `Unverified`, `Contradicted`, or `Unverifiable`. It runs in `veridikt_graph` after both layers are built and is a pure function of declared edges, derived edges, source text, and git metadata. Alongside it, **staleness detection** compares the git commit times of an annotation block against its subject's body and flags a block whose code moved on without it (`W0301`). Together they turn drift between a claim and reality from silent decay into a deterministic CI finding. (Spec §9.)

## Why

The entire value of Veridikt is that its answers can be trusted (guideline G-7). The declared layer asserts intent; the derived layer establishes fact; reconciliation is the only place those two meet, and it is what distinguishes Veridikt from every comment, doc, or wiki that asserts without ever being checked. It answers the project's number-one objection - "annotations rot" - directly:

- A claim that matches reality is **`Verified`**, and you can build on it.
- A claim the code actively contradicts is **`Contradicted`** and **fails CI** (`E0302` under strict, `W0302` otherwise), so the rot is caught at the commit that caused it.
- A claim that can neither be confirmed nor disproven is labeled honestly - `Unverified` or `Unverifiable` - never silently upgraded to fact (G-7: `Unverifiable` is an honest answer; print it).

Staleness exists because not every drift is a logical contradiction - sometimes the body simply changed and nobody revisited the intent. `W0301` surfaces exactly that, pointing at the commit that moved on (§9.2, and roadmap T7, "the trust milestone").

## Drawbacks

- **Only three claim kinds are reconciled in Phase 1.** `Affects`, `Reads`, and `Triggers` are checked against the derived layer. `Emits`, `Handles` (`on`), and `DependsOn` are `Unverifiable` in Phase 1 - event publication and dependency use are not derivable language-agnostically (§9.1). Total grounding for *every* claim kind arrives only with the Phase 2 `.veridikt` language, which is gated behind a milestone and not yet built.
- **`Contradicted` rests on a symbol-occurrence test, not on the heuristic edge.** A claim is `Contradicted` only when the function's subject span contains **zero token occurrences** of the target's bound host identifier. The `Heuristic` confidence of state-touch edges is deliberately *not* used to contradict - a heuristic miss must never poison a claim (§9.1, §8.4, G-7). The cost: a function that references the symbol but no longer truly writes it stays `Unverified`, not `Contradicted`. Veridikt chooses to under-report contradictions rather than risk a false one.
- **A target outside derivation scope is always `Unverifiable`.** If the claim's target lives in a file with no `derive`-tier language pack, the claim cannot be checked at all (§9.1). Mixed-language repos with a `scan`/`bind`-only language get honest non-answers for claims crossing into it.
- **Undeclared-effect detection is off by default.** A derived write from an *annotated* function with no matching `affects` declaration is `W0303`, but `[policy] undeclared_effects` defaults to `"off"` (§9.1, D-019). Because state writes are `Heuristic`, surfacing them by default would risk crying wolf; the guarantee exists but is opt-in. Unannotated functions are never penalized.
- **Staleness needs git, and is commit-time-based.** Outside a git work tree the check is skipped with a notice. Inside one, it compares max committer-time over block lines vs. subject-span lines (§9.2). A reordering commit that touches subject lines without changing meaning can therefore trip `W0301`; conversely, intent edited in the same commit as the code reads as fresh even if the human never re-thought it.
- **It is a snapshot, not a watcher.** Reconciliation recomputes per invocation from the current tree; it does not track a claim's status over time. Status lives in the run, surfaced in `lint`/`ask`/`show`.

## Architecture

Runs in `veridikt_graph` after both layers are assembled. It reads only data the CLI supplies - source text, the binder's host identifiers, and (for staleness) git metadata - never touching the filesystem or running git itself (§13, `ReconcileInput`).

**The four-status algorithm (§9.1, D-019).** For each declared `Affects` / `Reads` / `Triggers` edge `f → t`:

```
if t is outside derivation scope                         -> Unverifiable
else if a matching derived edge f -> t exists            -> Verified
else if f's subject span contains zero token occurrences
        of t's bound host identifier (token match,
        not substring)                                   -> Contradicted   (E0302 strict / W0302 warn)
else                                                     -> Unverified
```

A "matching derived edge" for `Triggers` is a derived `Calls` edge `f → t`; for `Affects`/`Reads`, a derived edge of the *same kind* `f → t`. `Emits`/`Handles`/`DependsOn` claims are `Unverifiable` in Phase 1. A claim whose target or source span text is unavailable can never be `Contradicted` - it stays `Unverified` (G-7).

**Undeclared effects (§9.1, D-067).** The graph always carries the base `W0303` warning for a derived `Affects` from an annotated function lacking the declaration; the `[policy] undeclared_effects` setting decides whether the lint surface shows it.

**Staleness (§9.2, D-018).** For each block, `t_subject` = max commit time over subject-span lines and `t_block` = max commit time over block lines, both via `git blame --line-porcelain`. If `t_subject > t_block`, emit `W0301`, reporting both timestamps and the subject's most recent commit hash. `veridikt lint` gathers the blame metadata (`--no-stale` skips it; `ask`/`stats` never gather it); `veridikt_graph::build` applies the comparison, so attribution and strict promotion behave like any graph finding. Promotable to error via `[policy] stale = "error"`.

**Where statuses appear.** Every status is visible in `veridikt lint`, in `veridikt ask` results, and in `veridikt show` node cards - each edge line carries its layer plus status (declared) or confidence (derived). `veridikt stats` reports the claim-status breakdown. Exit code `1` on any error-severity finding, including a `Contradicted` claim under strict.

## Usage Examples

Start from the annotated `charge` of spec §19, whose body genuinely writes `ledger` and reads `balances`:

```python
# @veridikt
# purpose: "Charge a customer"
# affects: Payment.ledger
# reads: Payment.balances
# emits: Payment.PaymentSettled
def charge(user_id, amount):
    if balances.get(user_id, 0) < amount:        # derived: Reads balances
        raise InsufficientFunds(user_id, amount)
    ledger.append(entry(user_id, amount))        # derived: Affects ledger
    publish(PAYMENT_SETTLED, user_id, amount)
```

`veridikt lint` here reconciles:

- `affects: Payment.ledger` → **Verified** (a matching derived `Affects` edge exists).
- `reads: Payment.balances` → **Verified**.
- `emits: Payment.PaymentSettled` → **Unverifiable** (event publication is not derivable in Phase 1).

**After** a refactor deletes the write but leaves the claim:

```python
# @veridikt
# affects: Payment.ledger        # claim unchanged...
# reads: Payment.balances
def charge(user_id, amount):
    if balances.get(user_id, 0) < amount:
        raise InsufficientFunds(user_id, amount)
    record_charge(user_id, amount)   # ...but `ledger` no longer appears in the body
```

The subject span now contains zero occurrences of `ledger`, so `affects: Payment.ledger` flips to **`Contradicted`** - `W0302`, or `E0302` if `Payment`'s module declares `enforcement: strict`, failing the build. Separately, the day the body changes without the block changing, the block trips **`W0301` stale-intent**, naming the subject's most recent commit.

Inspect any node's claim statuses directly:

```sh
veridikt ask 'show(Payment.charge)'   # each edge line: layer + status (declared) or confidence (derived)
veridikt stats                        # claims by status across the project
```

## Quickstart

Reconciliation runs automatically inside `veridikt lint` once a project has declared claims over `derive`-tier languages.

```sh
cd your-project
veridikt lint      # resolution + reconciliation; exit 1 on error-severity findings (incl. Contradicted)
veridikt stats     # claim-status breakdown: Verified / Unverified / Contradicted / Unverifiable
```

To see a `Contradicted` finding fail CI deliberately, promote the owning module to strict in an `@veridikt` scoping block or rely on the default `W0302`, then break a claim:

```python
# @veridikt
# affects: Payment.ledger
def refund(payment_id, amount):
    pass              # no occurrence of `ledger` -> affects claim is Contradicted
```

```sh
veridikt lint            # reports W0302 (or E0302 under enforcement: strict) for refund
```

Tune behavior in `veridikt.toml`:

```toml
[policy]
stale              = "warn"   # "warn" | "error"  - promote W0301 to a build failure
undeclared_effects = "off"    # "off"  | "warn"   - surface W0303 for annotated functions
```

Run `lint` with `--no-stale` to skip the git blame pass when you only want claim reconciliation. See the [declared](../declared-intent/README.md) and [derived](../derived-facts/README.md) layers for the two sides this pass compares.
