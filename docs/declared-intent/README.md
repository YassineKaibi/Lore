# The Declared Layer

## Description

The declared layer is Veridikt's *why*: the human knowledge that no static analysis can recover, written as `@veridikt` annotation blocks inside ordinary host-language line comments directly above the declaration they describe. A block carries prose clauses (`purpose`, `owner`, `because`, `assumes`, `unknown`) and reference clauses that become graph edges (`affects`, `reads`, `triggers`, `emits`, `on`, `depends_on`), plus structural clauses (`kind`, `name`, `route`, `enforcement`). The scanner finds the blocks, the binder attaches each to its subject declaration via tree-sitter, and the clause parser turns the text into a typed `Intent`. The result is the assertion side of the intent graph - claims a human makes about the code, subject to reconciliation. (Spec §5, §7.)

## Why

Comments, docs, and wikis are legible but untrustworthy - they live apart from the code and drift the moment the code changes. The declared layer exists to capture intent **in source, next to the declaration**, so it cannot live in a sidecar that silently falls out of sync (guideline G-8: in-source intent is load-bearing, never to be dropped). It records exactly the knowledge the derived layer cannot:

- **Purpose and rationale** (`purpose`, `because`) - why a construct exists and why a non-obvious decision was made.
- **Effect claims** (`affects`, `reads`, `triggers`) - assertions about what code touches, which the derived layer can check.
- **Honest gaps** (`unknown`, `assumes`) - what is untested, unresolved, or assumed about inputs and environment. `unknown` is surfaced by lint (`W0213`) so open questions are visible, not buried.
- **Ownership and boundaries** (`owner`, `depends_on`, `enforcement`) - responsibility, declared dependency surface, and per-module strictness.

Because declared and derived nodes share one table (open-world resolution, §7.6/D-017), a claim may reference a symbol that exists only as a derived node - the declared layer adds the *why* on top of facts it did not have to restate.

## Drawbacks

- **Line comments only.** A block is a run of contiguous line comments whose first content line is exactly `@veridikt`. Block comments (`/* */`, `""" """`) are **not** scanned in v1 - one rule, every language (§7.1). Codebases that prefer block-doc conventions must use line comments for intent.
- **Every cross-module reference must be fully qualified.** There is no implicit context: you write `affects: Payment.ledger`, never `affects: ledger`. An unresolved ref is `E0306`; a wrong-kind ref is `E0307` (§5.1, §6.3). This is deliberate (G-8) but verbose.
- **Binding is strict about what follows the block.** After the comment lines (and any blank lines), the next tree-sitter node must be a declaration from the per-language table (§7.4). Otherwise the block is `E0102` unbound. Multi-target declaration forms (e.g. multi-assignment) must carry an explicit `name:` or they are `E0104`.
- **Clause applicability is enforced and rigid.** The applicability matrix (§3.2) is normative: a clause illegal for a kind is `E0203`, a required clause missing is `E0201`, a workflow step with none of `triggers`/`emits`/`on` is `E0204`. You cannot, for example, put `affects` on a `Module` or `owner` locally on a `State` (it is inherited).
- **Some claims can never be verified in Phase 1.** `emits`, `on` (`Handles`), and `depends_on` claims are `Unverifiable` in Phase 1 because event publication and dependency use are not derivable language-agnostically (§5, §9.1). They are recorded honestly, not checked, until Phase 2.
- **A file outside every module is an orphan.** A file matched by no `[modules]` glob and carrying no top-of-file scoping block gives its subjects the qname `_orphan.<name>` and a `W0208` warning (§7.5). Module mapping is a setup prerequisite, not optional.

## Architecture

Crate: `veridikt_annotations` (scanner + binder), consuming the clause grammar from `veridikt_intent`. Input: a source tree. Output: blocks, their subjects, and qnames - handed as data to `veridikt_graph`.

**Scanning (§7.1, §7.2).** The scanner finds each `@veridikt` block, strips each line's comment token plus at most one following space, and reassembles the remainder into one intent-block text. A block grammar of `marker_line` (`@veridikt`) followed by binding lines (`kind:`, `name:`) and clause lines is parsed; default `kind` is `function`. An unknown clause is `E0202` (with the nearest valid clause suggested); a clause whose value violates its grammar is `E0207` and contributes nothing to the `Intent`.

**Binding (§7.3, D-013).** After the block, the binder skips blank lines and attaches to the next declaration node, descending through wrapper nodes from the per-language skip set (e.g. Python `decorated_definition`, TS `export_statement`, Rust `attribute_item`). The subject's host identifier comes from the declaration node's `name` field; `name:` in the block overrides it. Blocks of kind `module`/`service`/`workflow` are **scoping blocks**: they bind to no declaration, require an explicit `name:`, and scope their file.

**Module scoping (§7.5, D-015).** A subject's qname is `<module>.<name>`, where the module comes from a `veridikt.toml [modules]` glob (first match wins) or a top-of-file scoping block. Steps additionally require an enclosing `kind: workflow` block in order (`E0105`).

**The `Intent` contract (§13).** Parsing yields the shared `Intent` struct: singular prose clauses (`purpose`, `owner`) as `Option`, repeatable ones (`because`, `unknown`, `assumes`) as `Vec`, and reference clauses (`affects`, `reads`, `triggers`, `emits`, `on`, `depends_on`) as `Vec<Spanned<Ref>>` - refs unresolved at this stage (resolution happens in `veridikt_graph`, §6.3). This struct is consumed verbatim by both phases (G-3).

**Clause semantics (§5).** Reference clauses produce edges with required target kinds: `affects`/`reads` → `State`, `triggers` → `Function`, `emits`/`on` → `Event`, `depends_on` → `Module`/`Service`/`External`. `enforcement: strict` on a module promotes that module's warnings to errors. `route` marks HTTP entry points (legal on a function only inside a `Service`).

## Usage Examples

**Before** - a plain comment that drifts silently and connects to nothing Veridikt can check:

```python
# Charge a customer. Writes the ledger and reads balances.
def charge(user_id, amount):
    ...
```

**After** - the same intent as a structured, bindable, reconcilable block (real syntax, from spec §19):

```python
# @veridikt
# kind: state
# name: ledger
# purpose: "Append-only record of every money movement"
ledger = []

# @veridikt
# purpose: "Charge a customer"
# because: "Idempotency key is generated by the caller -- we do not deduplicate here"
# assumes: "amount is non-negative and already currency-validated"
# affects: Payment.ledger
# reads: Payment.balances
# emits: Payment.PaymentSettled
# unknown: "Behavior under concurrent charge + refund on one account is untested"
def charge(user_id, amount):
    if balances.get(user_id, 0) < amount:
        raise InsufficientFunds(user_id, amount)
    ledger.append(entry(user_id, amount))
    publish(PAYMENT_SETTLED, user_id, amount)
```

The first block is a **scoping/declaring** annotation: `ledger = []` is a value-binding form that derives no node on its own, so the block declares it as `kind: state` named `ledger`, giving it qname `Payment.ledger`. The second block binds to `charge` (qname `Payment.charge`) and asserts four reference claims plus prose. Inspect any node and its declared intent:

```sh
veridikt ask 'show(Payment.charge)'   # qname, kind, origin, every clause verbatim, edges, statuses
veridikt ask 'unknown'                # every declared open question in the project
veridikt ask 'owner("payments-team")' # nodes by declared owner
```

## Quickstart

```sh
cd your-project
veridikt init      # detect languages, write a starter veridikt.toml
```

Map your source to modules so subjects get qualified names:

```toml
[project]
name = "your-project"
languages = ["python", "typescript"]
roots = ["src"]

[modules]
"src/payments/**" = "Payment"
```

Add a block above a declaration - comment token of the host language, first content line exactly `@veridikt`:

```python
# @veridikt
# purpose: "Apply a refund and record it"
# affects: Payment.ledger
def refund(payment_id, amount):
    ledger.append(LedgerEntry(payment_id, -amount))
```

Then verify it scans, binds, and resolves:

```sh
veridikt scan      # lists the block, its subject, qname, kind
veridikt lint      # resolution + applicability findings; E0306 if Payment.ledger is unknown
```

If `lint` reports `E0306` for `Payment.ledger`, declare `ledger` as `kind: state` (as above) so the ref resolves. Once it does, the claim is ready for [reconciliation](../reconciliation/README.md).
