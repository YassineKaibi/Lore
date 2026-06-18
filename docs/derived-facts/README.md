# The Derived Layer

## Description

The derived layer is Veridikt's *what*: the set of nodes and edges extracted statically from host source code, with no human input. For every supported language it indexes declarations into graph nodes (functions, types), traces call expressions into `Calls` edges, and classifies occurrences of module state into `Affects` (write) and `Reads` (read) edges. Every derived edge is true by construction and carries an explicit confidence label - `Exact`, `Resolved`, or `Heuristic` - so a consumer always knows how a fact was established. The layer is available with zero annotations; it is the floor the declared layer is reconciled against. (Spec §8.)

## Why

Source code is the only fully trustworthy account of what a program does, but it is illegible at scale - you cannot read 50,000 lines to answer "what writes the ledger?" Veridikt's thesis is one intent graph with two layers; the derived layer exists so that the graph has a *ground truth* that no human asserts and no human can let rot. It enables three things the declared layer alone cannot:

- **Querying with zero annotations.** `veridikt ask 'triggers(X)'` works on an unannotated repo because the call graph is derived (§8.2, roadmap T6 exit criteria).
- **Open-world resolution.** A declared `affects: Payment.ledger` can name a state symbol that only exists as a derived node; the two layers share one node table (§7.6, D-017).
- **Reconciliation.** Every declared effect claim is checked against this layer - that comparison is the entire trust mechanism (§9, see [reconciliation](../reconciliation/README.md)).

The governing constraint is guideline **G-7: never present a guess as a fact.** The derived layer is built to be conservative on purpose - it would rather omit an edge than invent one.

## Drawbacks

These are real, current limitations, stated plainly. Most are deliberate consequences of G-7.

- **State touches are always `Heuristic`.** `Affects`/`Reads` edges are classified by pattern matching (assignment, augmented assignment, or a known mutator method such as Python `append`/`update` or TS `push`/`set`). The label is `Heuristic` *always*, because a pattern can misclassify (§8.3, §8.4). Absence of a heuristic edge never by itself produces a `Contradicted` status (§9.1, G-7).
- **Method-call resolution is intentionally narrow.** `obj.method(...)` resolves only when `obj` is a same-file/module class instance whose type is syntactically evident (direct construction in the same function). Otherwise the call is dropped. A dropped edge is invisible; a wrong edge is poison (§8.2).
- **Import resolution is v1-limited per language.** Python resolves `import`/`from … import` against `[project] roots` but **relative imports drop**. TypeScript resolves **relative imports only** (`./`, `../`); default imports drop, only named/namespace forms resolve. Aliases beyond one level, dynamic import, re-exports, star imports, and dotted callees deeper than `alias.name(...)` all fall through to "drop and count" (§8.2).
- **Unresolved calls disappear from view.** When a callee cannot be resolved, the edge is not created - it is only counted in `veridikt stats` as `unresolved_calls`. Coverage is therefore never 100%, and that is by design (§8.2 rule 3).
- **Value-binding forms derive no node.** Python `assignment` and TS `lexical_declaration`/`variable_declaration` produce no derived node; such symbols enter the graph only via annotation (§8.1, D-060a). A module-level constant is invisible until you declare it.
- **qnames are flat regardless of nesting.** A nested function's qname is `module + identifier`, ignoring its lexical nesting (§8.1, D-060b). Two same-named nested declarations in one module collide; colliding derived-only declarations are all dropped and counted as `ambiguous_derived_names` (§8.1, D-060d).
- **Block comments are never scanned.** Only line comments carry annotations (this affects the declared layer, but it shapes what the derived layer can be reconciled against - §7.1).

## Architecture

Crate: `veridikt_derive`. Input: the files in *derivation scope* (files assigned a module whose language has a `derive`-tier pack, §2/D-061). Output: a `DerivedLayer` of nodes, edges, and the scope set, handed as data to `veridikt_graph` - the graph never depends on the derive crate (§13).

**Derived nodes (§8.1).** Every declaration-table entry (§7.4) in scope becomes a node: functions/methods → `Function`; classes/structs/enums/interfaces/type aliases → `Type`. A derived node that collides with a declared node of the *same declaration* (same file, same start line) merges to origin `Both` with the declared kind/intent/loc winning - the normal annotated case.

**`Calls` edges (§8.2).** For each call expression in a function body, the callee is resolved in order:
1. **`Exact`** - callee is declared in the same file.
2. **`Resolved`** - callee is imported and the import resolves to a file inside derivation scope (per the pack's import strategies).
3. **Otherwise dropped**, counted as `unresolved_calls`.

Every call attributes to the nearest enclosing derived `Function`; a call with no such enclosing function (module-level, value-bound lambdas) is dropped.

**`Affects`/`Reads` edges (§8.3).** For host symbols bound to `State` nodes, an occurrence is a **write** if it matches the per-language mutator pattern set (assignment, augmented assignment, or a mutator method on the symbol) and a **read** otherwise. Confidence is `Heuristic`. Touches dedupe to one per `(function, state, kind)`.

**Confidence (§8.4).** `Exact` = same-file syntactic resolution. `Resolved` = cross-file via static import resolution, certain up to shadowing. `Heuristic` = pattern-based, may misclassify, never invented. Every surface that prints a derived edge prints its confidence.

**Language packs (§8.6).** From milestone T8, each language is a declarative **pack** (`packs/<lang>/`), not Rust code: a `veridikt-lang.toml` manifest plus `queries/bind.scm` and `queries/derive.scm` tree-sitter queries against a fixed capture vocabulary (`@subject.function`, `@call.callee`, `@touch.assign_lhs`, …), plus a mandatory conformance fixture suite. A pack declares a **tier** - `scan` (find blocks only), `bind` (also attach to subjects), or `derive` (also extract this layer). The engine does all resolution, attribution, and confidence labeling; the queries only say *where* the syntax is. Supported: Python, TypeScript, Rust, Go, Java (§8, D-070/D-071).

## Usage Examples

Consider an unannotated Python module under `[modules] "src/payments/**" = "Payment"`:

```python
# src/payments/service.py
def charge(user_id, amount):
    ledger.append(entry(user_id, amount))   # call: entry(...) in same file
    audit.record(charge)                     # call: audit.record - drops unless audit's type is evident
```

**Before any annotation**, the derived layer alone already supports call-graph queries:

```sh
veridikt ask 'triggers(Payment.charge)'   # functions Payment.charge calls - derived Calls edges
```

`entry` resolves `Exact` (same file). `audit.record` is a method call on a symbol whose type is not syntactically evident, so it **drops** and increments `unresolved_calls` rather than appearing as a guessed edge.

**State touches require a declared `State` node** to attribute against (the symbol must be bound to one - §8.3, D-060). Once `ledger` is declared as state (see the [declared layer](../declared-intent/README.md)), the same body derives an `Affects` edge:

```python
# After Payment.ledger is declared as kind: state
ledger.append(entry(user_id, amount))   # derived: Affects Payment.ledger (Heuristic)
```

Output never hides the label - a write detected this way prints `Affects (Heuristic)`, distinguishing it from an `Exact` call edge so a rendered graph or query result never launders a pattern guess into a certainty.

## Quickstart

The derived layer needs no annotations - point Veridikt at a project and inspect what it extracted.

```sh
cd your-project
veridikt init      # detect languages, write a starter veridikt.toml
veridikt stats     # nodes by kind/origin, plus unresolved_calls and ambiguous_derived_names
```

`veridikt.toml` must assign your source to modules (the derivation scope) and list languages:

```toml
[project]
name = "your-project"
languages = ["python", "typescript"]
roots = ["src"]

[modules]
"src/payments/**" = "Payment"
```

Then query the derived call graph directly - no `@veridikt` blocks required:

```sh
veridikt ask 'triggers(Payment.charge)'    # what charge calls (derived Calls)
veridikt ask 'show(Payment.charge)'        # the node card: derived edges with confidence
```

Read `unresolved_calls` in `veridikt stats` as the honest coverage gap: those are calls Veridikt refused to guess.
