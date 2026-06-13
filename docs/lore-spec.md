# Lore Specification

**Version:** 0.2 (Implementation-Ready)
**Status:** Authoritative. Supersedes v0.1 entirely.
**Companions:** `lore-decisions.md` (binding rationale, D-NNN refs), `lore-guidelines.md` (process), `lore-roadmap.md` (milestones).
**Normative language:** MUST / MUST NOT / SHOULD / MAY per RFC 2119. Anything non-normative is marked *Note*.

---

## Table of Contents

1. [Thesis and Architecture](#1-thesis-and-architecture)
2. [Glossary](#2-glossary)
3. [The Intent Model](#3-the-intent-model)
4. [Node Kinds](#4-node-kinds)
5. [Intent Clauses](#5-intent-clauses)
6. [The Graph: Edges, Resolution, Traversal](#6-the-graph)
7. [Phase 1: Annotations](#7-phase-1-annotations)
8. [Phase 1: The Derived Layer](#8-phase-1-the-derived-layer)
9. [Reconciliation and Staleness](#9-reconciliation-and-staleness)
10. [Query Language](#10-query-language)
11. [Project Manifest: lore.toml](#11-project-manifest-loretoml)
12. [CLI](#12-cli)
13. [Shared Data Contracts (Rust)](#13-shared-data-contracts)
14. [Phase 2: The Lore Language](#14-phase-2-the-lore-language)
15. [Phase 2: Formal Grammar](#15-phase-2-formal-grammar)
16. [Phase 2: Semantic Rules](#16-phase-2-semantic-rules)
17. [Phase 2: Execution Model](#17-phase-2-execution-model)
18. [Diagnostics Registry](#18-diagnostics-registry)
19. [Canonical Example](#19-canonical-example)

---

## 1. Thesis and Architecture

A developer has never been able to ask their system a question and trust the answer. Source code is the truth but illegible at scale; artifacts about code (comments, docs, wikis) are legible but untrustworthy. Lore ends that trade-off by maintaining **one intent graph with two layers**:

- **Derived layer** -- the *what*. Built by static extraction from host source (Phase 1) or by the compiler (Phase 2). True by construction, available with zero annotations. (D-001, D-002)
- **Declared layer** -- the *why*. Built from `@lore` annotation blocks (Phase 1) or `.lore` intent blocks (Phase 2). Human knowledge that no analysis can derive: `purpose`, `because`, `unknown`, `assumes`, `owner`.
- **Reconciliation** -- the honesty mechanism. Every declared effect edge is checked against the derived layer and labeled `Verified`, `Unverified`, `Contradicted`, or `Unverifiable`. Drift is a CI finding, not a silent decay. (D-019)

**Phases.** Phase 1 is the language-agnostic tool: scan annotations, derive structure, reconcile, answer `lore ask`, lint in CI, export, serve MCP. Phase 2 is the dedicated `.lore` language in which grounding is total: the compiler checks every effect declaration against actual state-API call sites. Phase 2 MUST NOT begin before the T10 gate passes (D-039).

**Governing design principle.** Every feature must strengthen the intent graph: add nodes, add edges, lower the cost of declaring them, or raise the trustworthiness of existing ones. Features that do none of these are out (deployment, scaling, orchestration remain explicitly out of scope).

---

## 2. Glossary

| Term | Definition |
|---|---|
| **Node** | A vertex in the intent graph. Has a `qname`, `Kind`, optional `Intent`, `SourceLoc`, and an origin (`Declared`, `Derived`, or `Both`). |
| **qname** | Qualified name: dot-separated segments, e.g. `Payment.ledger`. Globally unique (D-016). |
| **Subject** | The host-language declaration an annotation block binds to. |
| **Subject span** | The source line range of the subject declaration, including its body, as reported by tree-sitter. |
| **Edge** | Directed, typed connection between nodes. Carries `EdgeKind`, origin layer, `ClaimStatus` (declared edges), `Confidence` (derived edges), `SourceLoc`. |
| **Claim** | A declared edge; an assertion by a human, subject to reconciliation. |
| **Derivation scope** | The set of files assigned a module by either §7.5 mechanism whose language has derived-layer support (D-061). |
| **Intent block** | The run of intent clauses attached to a node (between comment marker and subject in P1; between signature and `{` in P2). |

---

## 3. The Intent Model

The intent model is shared verbatim between phases (`lore_intent` crate). One clause grammar, one AST, one graph. Phase 1 delivers clauses inside host-language comments; Phase 2 delivers them inside `.lore` syntax. Nothing else differs.

### 3.1 Clause set (final -- D-004..D-008)

`purpose`, `owner`, `because`, `unknown`, `assumes`, `affects`, `reads`, `triggers`, `emits`, `on`, `depends_on`, `route`, `enforcement`.

Removed from v0.1: `version`, `changed` (VCS-derived instead; see `lore history`, D-004).

### 3.2 Clause applicability and requirement matrix (normative)

Legend: **R** required, **O** optional, **Rec** recommended (lint `W0209` if absent), **--** illegal (`E0203` if present).

| Clause | Module | Service | Workflow | Step | State | Event | Type | Error | Function | External |
|---|---|---|---|---|---|---|---|---|---|---|
| `purpose` | R | R | R | O | Rec | R | O | -- | O | R |
| `owner` | R | R | R | -- | inherit | inherit | -- | -- | O | R |
| `because` | O | O | O | O | O | O | O | R (field) | O | O |
| `unknown` | O | O | O | O | O | O | O | -- | O | O |
| `assumes` | -- | -- | O | O | -- | -- | -- | -- | O | O |
| `affects` | -- | -- | -- | O | -- | -- | -- | -- | O | -- |
| `reads` | -- | -- | -- | O | -- | -- | -- | -- | O | -- |
| `triggers` | -- | -- | -- | O | -- | -- | -- | -- | O | -- |
| `emits` | -- | -- | -- | O | -- | -- | -- | -- | O | -- |
| `on` | -- | -- | -- | O | -- | -- | -- | -- | O | -- |
| `depends_on` | O | O | O | -- | -- | -- | -- | -- | -- | -- |
| `route` | -- | R (base) | -- | -- | -- | -- | -- | -- | O (in service) | -- |
| `enforcement` | O | -- | -- | -- | -- | -- | -- | -- | -- | -- |

Rules attached to the matrix:
- A workflow `step` MUST declare at least one of `triggers`, `emits`, `on` (`E0204`: empty step).
- `route` on a function is legal only when the function's parent is a `Service` (`E0205`).
- `owner` on State/Event is inherited from the owning module and MUST NOT be declared locally (`E0203`).
- "R" is enforced by `lore lint` in Phase 1 (`E0201` missing required intent) and by the compiler in Phase 2.
- Requirement rows (R / Rec) apply only to nodes declared by an intent block; nodes that exist only via a `lore.toml` mapping or the derived layer are exempt (D-046).

---

## 4. Node Kinds

```
Kind ::= Module | Service | Workflow | Step | State | Event | Type | Error | Function | External
```

| Kind | Meaning | Phase 1 origin | Phase 2 construct |
|---|---|---|---|
| Module | Concept boundary; owns members | `kind: module` / lore.toml mapping | `def module` |
| Service | Runnable unit serving routes | `kind: service` | `def service` |
| Workflow | Named ordered business process | `kind: workflow` | `def workflow` |
| Step | One stage of a workflow | `kind: step` (must be under a workflow) | `step` |
| State | Named, typed, module-owned mutable state | `kind: state` | `def state` |
| Event | Named occurrence with typed payload | `kind: event` | `def event` |
| Type | Data shape | `kind: type`; also derived | `def type` |
| Error | Typed failure mode | `kind: error` | `def error` |
| Function | Behavior | `kind: function` (default); also derived | `def` |
| External | Typed boundary to a non-Lore dependency (D-009) | `kind: external` | `def external` |

A node's origin is `Declared` (annotation only), `Derived` (extraction only), or `Both` (an annotation bound to a declaration the derived layer also indexed -- the normal case).

---

## 5. Intent Clauses

Exact semantics per clause. "Ref" clauses produce edges; "prose" clauses annotate the node.

**`purpose: "<string>"`** -- what the construct exists for. Prose. One occurrence max (`E0206` on repeat).

**`owner: "<string>"`** -- responsible team. Prose, queryable. One occurrence. Cross-checked against CODEOWNERS when present (`W0207`, D-010; mechanics per D-058).

**`because: "<string>"`** -- why a non-obvious decision was made. Prose. Repeatable. On `Error` it is a required field of the error definition, not an intent clause.

**`unknown: "<string>"`** -- what is unresolved, untested, or not understood. Prose. Repeatable. Each occurrence is surfaced by lint as `W0213`; severity per `[policy] unknown` (D-012, D-057).

**`assumes: "<string>"`** -- what must be true of inputs or environment on entry (D-005). Prose. Repeatable. No resolution, no edges.

**`affects: ref, ref, ...`** -- declared **writes**. Each ref MUST resolve to a `State` node (`E0307` otherwise). Edge kind `Affects`. Reconciled (D-019).

**`reads: ref, ...`** -- declared **reads** (D-006). Refs resolve to `State`. Edge kind `Reads`. Reconciled.

**`triggers: ref, ...`** -- declared **cross-module synchronous calls** (D-007). Refs resolve to `Function`. Same-module target → `W0205`. Edge kind `Triggers`. Reconciled.

**`emits: ref, ...`** -- declared event publication. Refs resolve to `Event`. Edge kind `Emits` (function/step → event). Not reconciled in Phase 1 (event publication is not derivable language-agnostically) → claims are `Unverifiable` unless Phase 2.

**`on: ref, ...`** -- declared event handling. Refs resolve to `Event`. Edge kind `Handles` (function/step → event).

**`depends_on: ref, ...`** -- asserted dependency surface (D-008). Refs resolve to `Module | Service | External`. Edge kind `DependsOn`. Both directions linted: undeclared use `E0304`, unused declaration `W0206`.

**`route: METHOD "<path>"` | `route: "<path>"`** -- HTTP entry point. The bare-string form is the service base path; the METHOD form marks a function as a handler. Marks the node as a graph entry point (used by `path`/`reaches` reporting).

**`enforcement: strict | warn`** -- module policy (D-011). Phase 1: findings inside the module at `strict` are errors. Phase 2: compile errors. Default `warn`. Not inherited by nested modules.

### 5.1 Ref syntax

```
ref ::= Ident ("." Ident)*
```
Refs are unresolved at parse time (`lore_intent` stores segments + span); resolution happens in `lore_graph` (§6.3). All cross-module refs MUST be fully qualified -- there is no implicit context in either phase.

---

## 6. The Graph

### 6.1 Edge kinds

```
EdgeKind ::= Affects | Reads | Triggers | Emits | Handles | DependsOn | Contains | Sequence | Calls
```

- `Contains` -- structural: Module→member, Service→function, Workflow→Step. Always derived from structure; never declared. The container is the longest proper qname prefix of kind Module/Service/Workflow (D-047).
- `Sequence` -- Step→next Step, from declaration order.
- Structural `Contains`/`Sequence` edges carry layer `Derived` with confidence `Exact` (D-047).
- `Calls` -- derived-layer call edge, function→function (any module). Declared `Triggers` claims reconcile against `Calls`.
- All others as in §5.

### 6.2 Edge record

Every edge carries: `from: qname`, `to: qname`, `kind: EdgeKind`, `layer: Declared | Derived`, `loc: SourceLoc`, and exactly one of:
- declared edges: `status: ClaimStatus` (`Verified | Unverified | Contradicted | Unverifiable`), assigned by reconciliation;
- derived edges: `confidence: Exact | Resolved | Heuristic` (§8.4, D-020).

### 6.3 Resolution (runs in `lore_graph`, shared by both phases)

For every declared ref:
1. Look up the qname in the node table (declared ∪ derived nodes, D-017). Missing → `E0306` unresolved ref (surfaced by lint; `E0306` findings in `Graph.findings` are the canonical representation, D-047; the edge is not created).
2. Check the target kind against the clause's required kind (table below). Mismatch → `E0307` with a message naming both kinds; the edge is not created (D-047).

| Clause | Required target kind |
|---|---|
| `affects`, `reads` | `State` |
| `triggers` | `Function` |
| `emits`, `on` | `Event` |
| `depends_on` | `Module`, `Service`, `External` |

3. Required-intent-by-kind (matrix §3.2) is checked here (`E0201`), as are duplicate qnames (`E0305`) and the `depends_on` surface rules (`E0304`/`W0206`; effective depends_on, owner chains, and module-locality per D-048).

### 6.4 Traversal matrix (normative -- D-024)

The **event hop** is the composite traversal `X --Emits--> Event <--Handles-- Y`, yielding X→Y causality. Reverse traversal inverts every arrow.

| Query | Direction from argument | Edge kinds traversed | Transitive set (with `*`) |
|---|---|---|---|
| `affects(S)` | reverse | one `Affects` into S | prepend reverse `Triggers`/`Calls` and reverse event-hop chains |
| `reads(S)` | reverse | one `Reads` into S | same as above |
| `touches(F)` | forward | `Affects` ∪ `Reads` out of F | append forward `Triggers`/`Calls`/event-hop chains |
| `triggers(F)` | reverse | `Triggers` ∪ `Calls` into F | transitive over same |
| `emits(E)` | reverse | `Emits` into E | n/a (no `*`) |
| `handlers(E)` | reverse | `Handles` into E | n/a |
| `depends(X)` | forward | `DependsOn` | transitive |
| `dependents(X)` | reverse | `DependsOn` | transitive |
| `reaches(X)` | forward | `Contains`, `Sequence`, `Triggers`, `Calls`, event hop, `Affects`, `Reads`, `DependsOn` | always transitive |
| `path(A, B)` | forward from A | same set as `reaches` | shortest path; `--all --max-len N` for all simple paths |
| `tagged("t")` | -- | node attribute scan | n/a |
| `owner("t")`, `unknown in ...`, `show(X)` | -- | node scans / single card | n/a |

Derived `Calls` edges participate in every traversal where `Triggers` does; results MUST label which hops were declared vs derived and at what confidence.

---

## 7. Phase 1: Annotations

### 7.1 The `@lore` block

A block is a run of **contiguous** host-language line comments whose first content line is exactly `@lore`. The scanner strips each line's comment token plus at most one following space, preserves newlines, and reassembles the remainder into one intent-block text. Blank line or non-comment line ends the block.

```python
# @lore
# kind: state
# purpose: "Append-only record of every money movement"
ledger = []
```

Block-level comments (`/* */`, `""" """`) are NOT scanned in v1 -- line comments only (one rule, every language).

### 7.2 Block grammar

```ebnf
lore_block    ::= marker_line ( binding_line | clause_line )*
marker_line   ::= "@lore"
binding_line  ::= kind_field | name_field
kind_field    ::= "kind" ":" ( "module" | "service" | "workflow" | "step"
                             | "state" | "event" | "type" | "error"
                             | "function" | "external" )
name_field    ::= "name" ":" Ident ( "." Ident )*
clause_line   ::= intent_clause          (* §15 production, shared parser *)
```
Default `kind` is `function`. A quoted string MAY span lines (the scanner reassembles before the clause parser runs); a clause otherwise ends at the newline. Unknown clause names → `E0202` with the nearest valid clause suggested. A clause whose value does not match its §15 production is `E0207` malformed clause, and contributes nothing to the block's `Intent` (D-045). An invalid or duplicate `kind:` value is `E0106`; an invalid or duplicate `name:` value is `E0107` (D-041).

### 7.3 Binding (D-013)

After the block's last comment line, skip zero or more blank lines. The next tree-sitter node MUST be a declaration node from the per-language table (§7.4), possibly nested inside wrapper nodes from the skip set. Otherwise: `E0102` unbound annotation, with the offending node type in the message. When several nodes start on the subject line (e.g. a Python class-body `block` and its first `function_definition`), the binder takes the first declaration node along the descent path, shallowest first (D-044).

The subject's **host identifier** is the text of the declaration node's `name` field (tree-sitter field name per table; exact per-node fields in D-042). `name:` in the block overrides it. Declaration forms with multiple targets or declarators MUST carry an explicit `name:` (`E0104`).

Blocks of kind `module`, `service`, or `workflow` are **scoping blocks** (§7.5): they bind to no declaration, are exempt from `E0102`, and MUST carry an explicit `name:` (`E0108`) (D-042).

### 7.4 Per-language tables (normative; grammar = official tree-sitter grammar)

| Language | Comment token | Declaration node types | Wrapper/skip node types |
|---|---|---|---|
| Python | `#` | `function_definition`, `class_definition`, `assignment` (module/class level) | `decorated_definition` (descend), `expression_statement` (descend), `decorator` |
| TypeScript/TSX/JS | `//` | `function_declaration`, `class_declaration`, `method_definition`, `lexical_declaration`, `variable_declaration`, `interface_declaration`, `type_alias_declaration`, `enum_declaration` | `export_statement` (descend), `decorator` |
| Go | `//` | `function_declaration`, `method_declaration`, `type_declaration`, `var_declaration`, `const_declaration` | -- |
| Java | `//` | `method_declaration`, `constructor_declaration`, `class_declaration`, `interface_declaration`, `enum_declaration`, `record_declaration`, `field_declaration` | `marker_annotation`, `annotation` |
| Rust | `//` | `function_item`, `struct_item`, `enum_item`, `trait_item`, `static_item`, `const_item`, `mod_item` | `attribute_item` |

Adding a language = adding one row here plus a language pack realizing it (§8.6; before T8, per-language code per §8.5). The roadmap fixes the order (D-014). This table remains normative under packs: a builtin pack MUST realize exactly its row's values, enforced by its conformance fixtures (D-070). Subject identifiers come from the per-node tree-sitter fields listed in D-042 (Python, TypeScript) and D-050 (Rust), carried from T8 as `@subject.name` captures in the pack's `bind.scm` (§8.6.3). Rust's `attribute_item` is a preceding-sibling skip, not a descend wrapper (D-050).

### 7.5 Module scoping (D-015)

1. `lore.toml [modules]` maps path globs → module names. Every file matching a glob belongs to that module. Overlapping globs assigning two different modules to one file: `E0103`.
2. A top-of-file block with `kind: module` (or `service`/`workflow`) scopes that file and overrides (1) for that file.
3. A file in neither: annotated subjects get qname `_orphan.<name>` and `W0208`.

Subject qname = `<module name>.<name>`. Steps additionally require an enclosing `kind: workflow` block in the same file, in order (`E0105` otherwise); step qname = `<workflow qname>.<step name>`, where a file-scoping workflow block's qname is its bare name (D-042).

### 7.6 Closed vs open world

Declared refs resolve against declared ∪ derived nodes (D-017). The old "annotated subjects only" rule is dead: you can `affects: Payment.ledger` where `ledger` is a derived-only node, and the answer to `lore ask show(Payment.ledger)` will simply show no declared intent yet.

---

## 8. Phase 1: The Derived Layer

Crate: `lore_derive`. Input: the files in derivation scope. Output: derived nodes and edges with confidence labels. v1 languages: Python, TypeScript (T6); Go, Java, Rust (T8).

### 8.1 Derived nodes

Every declaration-table node (§7.4) in scope becomes a derived node: functions/methods → `Function`, classes/structs/enums/interfaces/type aliases → `Type`; value-binding forms (Python `assignment`, TS `lexical_declaration`/`variable_declaration`) derive no node and enter the graph only via annotation (D-060a). Nested functions and methods derive too. qname = module (from §7.5) + host identifier, flat regardless of nesting (D-060b). A derived node that collides with a declared node of the same qname **and the same declaration** (same file, same declaration start line) merges into one node with origin `Both` (this is the normal annotated case); the declared kind, intent, and loc win. A collision with a declared node of a *different* declaration is `E0305` (the derived node and its edges are dropped). Collisions among derived-only declarations produce no finding: all colliding declarations and their edges are excluded and counted in `lore stats` as `ambiguous_derived_names` (D-060d).

### 8.2 Derived `Calls` edges

For every call expression inside a function body, resolve the callee:
1. **Exact:** callee identifier is declared in the same file. Confidence `Exact`.
2. **Resolved:** callee is imported, and the import resolves (per-language rules below) to a file inside derivation scope. Confidence `Resolved`.
3. Otherwise: **dropped**, counted in `lore stats` as `unresolved_calls`. Never guess (D-020).

Import resolution v1: Python -- `import m [as a]` / `from m import n [as a]` against the project source root(s) in `lore.toml [project] roots` (relative imports drop); TypeScript -- relative imports only (`./`, `../`, resolving `<p>.ts|.tsx|.js` or `<p>/index.ts`), named (`{ n [as a] }`) and namespace (`* as m`) forms (default imports drop); Go -- same-package files + intra-module import paths; Java -- same package + explicit single-type imports; Rust -- `use crate::...` paths within the workspace. Anything else (aliases beyond one level, dynamic import, re-exports, star imports, dotted callees deeper than `alias.name(...)`) is out of v1 scope and falls to rule 3 (D-062c). From T8 these rules are realized as pack-selected strategies from the built-in strategy library (§8.6.1, D-071); the semantics above are unchanged.

Every call and state touch attributes to the nearest enclosing derived `Function` node; a call with no such node (module/class level, value-bound lambdas, declarations dropped per D-060d), or one resolving to a non-`Function` node, is dropped and counted in `unresolved_calls` (D-062a/b).

Method calls (`obj.method(...)`) resolve only when `obj` is a same-file/module class instance whose type is syntactically evident (direct construction in the same function); otherwise dropped. *Note: this is deliberately conservative -- a dropped edge is invisible, a wrong edge is poison.*

### 8.3 Derived `Affects`/`Reads` edges (state touches)

Targets: host symbols bound to `State` nodes. Scope: files of the owning module plus files whose imports resolve to it.

- **Write** if the symbol occurrence matches the per-language mutator pattern set: assignment / augmented assignment to the symbol, or a method call on the symbol from the mutator list (Python list/dict/set: `append extend insert remove pop clear update setdefault sort reverse add discard`; TS array/map/set: `push pop shift unshift splice sort reverse fill set delete clear add`; Go: assignment, `append(sym, ...)` re-assigned, map index assignment, `delete(sym, ...)`; Java: assignment plus `add addAll put putAll remove clear set sort`; Rust: assignment, `push insert remove clear extend sort` on the symbol or `&mut` borrow passed onward).
- **Read** for any other occurrence of the symbol.
- Confidence: `Heuristic`, always.

Occurrence matching (D-062d): the symbol matches bare in same-language files of the owning module, bare via a named import of it, or as `alias.identifier` via a whole-module/namespace import -- the import forms only when the import resolves to the state's defining file. Occurrences with no enclosing derived function produce no edge (nothing to attribute). Touch edges dedupe to one per (function, state, kind); the first occurrence's span wins.

### 8.4 Confidence (D-020)

| Level | Meaning |
|---|---|
| `Exact` | Same-file syntactic resolution; effectively certain. |
| `Resolved` | Cross-file via static import resolution; certain up to shadowing. |
| `Heuristic` | Pattern-based classification; may misclassify, never invented. |

Every surface that prints a derived edge MUST print its confidence. `Heuristic` absence alone MUST NOT produce a `Contradicted` status (§9.1 uses the symbol-occurrence test instead).

### 8.5 Language support artifacts

Each language contributes: (a) a tree-sitter grammar dependency, (b) declaration queries (`.scm`), (c) call-expression queries, (d) import-resolution rules, (e) a mutator list. From T8 these five artifacts are packaged as a **language pack** (§8.6, D-070): one `lore-lang.toml` plus two `.scm` query files plus a mandatory conformance fixture suite, loaded through one generic adapter -- there is no per-language Rust code (sole exception: named custom import strategies, D-071). The pre-T8 layout (`lore_derive/src/lang/<name>/`) is superseded; Python and TypeScript migrate onto packs at T8 with behavior unchanged (D-070h).

The parse cache (§10.7, G-9) stores per-file extraction facts keyed by content hash, not serialized trees; all cross-file resolution is recomputed from facts every run (D-064). The cache key includes the pack identity -- name, format version, content hash of the pack files -- so editing a pack invalidates its facts (D-070i).

### 8.6 Language packs (normative from T8 -- D-070, D-071)

A supported language is defined by a language pack. Builtin packs live at `packs/<name>/` in the workspace, are embedded into the binary at build time, and use statically linked tree-sitter grammars (workspace-pinned). Layout:

```
packs/go/
  lore-lang.toml        # pack manifest (§8.6.1)
  queries/bind.scm      # declaration recognition + identifiers (tier bind+)
  queries/derive.scm    # calls, imports, state-touch forms (tier derive)
  fixtures/             # mandatory conformance suite (§8.6.4)
```

`lore_cli` parses and validates packs and passes them to `lore_annotations` / `lore_derive` as data plus a grammar handle; the adapters compile the query files at activation (D-070d). A pack that fails any check in this section MUST NOT be activated -- the language is simply not loaded, never partially.

#### 8.6.1 Pack manifest: `lore-lang.toml`

Normative keys. An unknown key, missing required key, invalid value, tier/artifact mismatch, or an extension claimed by two loaded packs is `E0410`.

| Key | Required | Meaning |
|---|---|---|
| `[pack] name` | R | Language id; MUST equal the pack directory name and is the value used in `lore.toml [project] languages`. |
| `[pack] format` | R | Integer pack-format version. This spec defines version **1**. An unknown version is `E0412`; the pack is refused before anything else is read. |
| `[pack] tier` | R | `"scan"` \| `"bind"` \| `"derive"` (§8.6.2). |
| `[grammar] source` | R (bind+) | `"builtin"` in v1. `"wasm"` is reserved key space: recognized and refused with `E0413` -- later acceptance widens a value domain, never changes the format. |
| `[grammar] name` | R when `source = "builtin"` | Id of the statically linked grammar (e.g. `tree_sitter_go`); unknown id is `E0413`. |
| `[grammar] path` | reserved | WASM grammar file, used with `source = "wasm"`. |
| `[scanner] extensions` | R | File extensions the pack claims (e.g. `[".go"]`). |
| `[scanner] comment_token` | R | Line-comment token (§7.1). |
| `[binder] wrappers` | O (bind+) | Descend-wrapper node types (§7.3, D-042). |
| `[binder] sibling_skips` | O (bind+) | Preceding-sibling skip node types (D-050c). |
| `[derive] mutators.methods` | O (derive) | Method names that mutate a receiver state symbol (§8.3). |
| `[derive] mutators.free_functions` | O (derive) | Free functions that mutate their first state-symbol argument (§8.3; e.g. Go `delete`). |
| `[[derive.imports.strategy]]` | R (derive) | Ordered strategy stanzas: `kind` = `"relative"` \| `"root_relative"` \| `"package_dir"` \| `"manifest_prefix"` \| `"custom"` plus per-kind params (D-071). Tried in order; first resolution wins; no resolution -> drop and count (§8.2 rule 3). |

Example (Go):

```toml
[pack]
name   = "go"
format = 1
tier   = "derive"

[grammar]
source = "builtin"
name   = "tree_sitter_go"

[scanner]
extensions    = [".go"]
comment_token = "//"

[derive.mutators]
free_functions = ["delete"]

[[derive.imports.strategy]]
kind = "package_dir"
extensions = [".go"]

[[derive.imports.strategy]]
kind = "manifest_prefix"
manifest_file = "go.mod"
prefix_key = "module"
```

#### 8.6.2 Tiers

Cumulative; the declared tier MUST match the artifacts present in both directions (`E0410`).

- **`scan`** -- scanner only; no grammar. Scoping blocks (§7.5) work in full. A non-scoping block MUST carry `name:` (`E0109`) and binds to no subject: qname = §7.5 module + name, loc = the block span, no subject span -- so it is never staleness-checked (D-068d) and its claims can never be `Contradicted` (D-066c). Files never enter derivation scope.
- **`bind`** -- adds `[grammar]` + `bind.scm`: full §7 binding (subjects, host identifiers, `E0102`, subject spans -- staleness and the §9.1 occurrence test apply).
- **`derive`** -- adds `derive.scm`, import strategies, and mutator lists: files enter derivation scope (§2, D-061) and §8.1--§8.4 apply.

*Note: a tier is the honest statement of roadmap risk 1's "ship it scanner+binder-only and say so" -- declared in data, surfaced by `lore stats`.*

#### 8.6.3 Query files and capture vocabulary

The generic adapter understands a fixed capture vocabulary; any other capture name is `E0411`.

**`bind.scm`** (tier bind+): each pattern marks a §7.4 declaration node with exactly one of `@subject.function` (derives `Function`), `@subject.type` (derives `Type`), or `@subject.value` (value-binding form: bindable, derives no node -- D-060a), and captures its identifier as `@subject.name` (the §7.3 host identifier). A form whose pattern cannot capture a single `@subject.name` (multi-target forms) requires an explicit `name:` in the block (`E0104`).

**`derive.scm`** (tier derive): `@call` with either `@call.callee` (a bare call: the callee identifier) or `@call.receiver` + `@call.method` (a member call `recv.method(...)`, decomposed into the object and member identifiers); a `@call` with neither is opaque and drops (§8.2 rule 3). `@call.construct` marks a local construction binding (`x = Cls()` / `const x = new Cls(...)`) with `@call.construct.var` (bound variable) and `@call.construct.class` (constructed class), feeding the D-062e method-call rule (§8.2 resolution applies to all of these; D-072). `@import` with `@import.source`, `@import.name`, `@import.alias`, `@import.namespace` -- import forms, where **uncaptured forms drop**: the v1 exclusions of §8.2 (default imports, star imports, ...) are expressed by not capturing them, never by adapter special cases; `@touch.assign_lhs`, `@touch.aug_assign_lhs`, `@touch.receiver`, `@touch.call_function` -- state-touch classification sites, combined with the mutator lists per §8.3 (lhs/receiver token equal to a state symbol -> write; any other matched occurrence -> read; confidence `Heuristic` always).

Binder mechanics (D-042/D-044 wrapper descent and same-row search, D-050 sibling skips), the drop rules, and confidence labeling (§8.2--§8.4) are engine behavior, identical for every pack; a pack supplies only the data above.

#### 8.6.4 Conformance protocol

Mandatory; this is how packs protect G-7. `fixtures/<class>/<case>/` holds input source files plus an `expected.json` carrying the exact expected output (block sets / derived nodes and edges with confidences / findings -- the same shapes as the corresponding `--json` outputs). Mandatory classes by tier:

| Tier | Class | MUST include |
|---|---|---|
| scan+ | `scan` | Exact block sets; >=1 negative: comment text that MUST NOT scan as a block (e.g. block comments, §7.1). |
| bind+ | `bind` | >=1 positive per `@subject.*` capture kind used; wrapper/skip exercise when declared; >=1 negative producing `E0102`. |
| derive | `derive` | Exact derived node/edge sets with confidences; >=1 required absence (a call that MUST be dropped); per configured import strategy, >=1 resolved and >=1 dropped import; >=1 write and >=1 read state touch; >=1 negative occurrence that MUST NOT classify as a write. |

A pack missing a mandatory class (or with an empty one), or whose suite has not passed for its exact content, MUST NOT be activated (`E0415`). Builtin packs are enforced in CI by the **conformance harness** -- a `lore_cli` test running every embedded pack's suite through the real scan→bind→derive pipeline -- so a failing pack cannot ship. Future external packs (reserved, like WASM grammars) run the suite at first load.

---

## 9. Reconciliation and Staleness

Runs in `lore_graph` after both layers are built. Pure function of (declared edges, derived edges, source text, git metadata).

### 9.1 Claim statuses (D-019)

For each declared `Affects` / `Reads` / `Triggers` edge f→t:

```
if t outside derivation scope                       -> Unverifiable
else if matching derived edge exists                -> Verified
else if f's subject span contains zero occurrences
        of t's bound host identifier (token match,
        not substring)                              -> Contradicted   (E0302 strict / W0302 warn)
else                                                -> Unverified
```

`Emits`/`Handles`/`DependsOn` claims: `Unverifiable` in Phase 1 (Phase 2 verifies all of them). "Matching derived edge" for `Triggers` means a derived `Calls` edge f→t; for `Affects`/`Reads`, a derived edge of the same kind f→t. Test inputs, the per-target bound host identifier, the token-match definition, and the `E0302`/`W0302` code choice are per D-066; a target (or claim source) whose symbol or span text is unavailable can never be `Contradicted` -- the claim stays `Unverified` (G-7).

**Undeclared effects:** a derived `Affects` edge from an *annotated* function with no corresponding declaration → `W0303`, default off via `[policy] undeclared_effects = "off"` (D-019; mechanics per D-067 -- the graph always carries the base Warning, the policy applies at the lint surface). Unannotated functions are never penalized.

### 9.2 Staleness (D-018)

For each annotation block: let `t_subject = max(commit time over subject-span lines)`, `t_block = max(commit time over block lines)`, both via `git blame --line-porcelain`. If `t_subject > t_block` → `W0301 stale-intent`, reporting both timestamps and the subject's most recent commit hash. Outside a git work tree: check skipped, one notice line. Promotable to error via `[policy] stale = "error"`. Mechanics per D-068: `lore lint` gathers the blame metadata (committer-time; `--no-stale` skips; `ask`/`stats` never gather) and `lore_graph::build` applies the comparison, so attribution and strict promotion work like any graph finding.

### 9.3 `lore history <qname>`

Renders `git log -L <span_start>,<span_end>:<file>` for the node's subject span: hash, date, author, full message. This replaces the removed `changed:` clause (D-004). Mechanics per D-059: the qname MUST name a graph node (exit 2 with the nearest existing qname otherwise, mirroring D-053a); outside a git work tree or on git failure, exit 2 with the underlying message; an empty log is an honest empty answer (exit 0). Supports `--json` (commit array: full hash, author, ISO-strict author date, full message).

---

## 10. Query Language

### 10.1 Grammar

```ebnf
query        ::= query_expr filter*
query_expr   ::= "affects"  ["*"] "(" ref ")"
               | "reads"    ["*"] "(" ref ")"
               | "touches"  ["*"] "(" ref ")"
               | "triggers" ["*"] "(" ref ")"
               | "emits" "(" ref ")"
               | "handlers" "(" ref ")"
               | "depends"    ["*"] "(" ref ")"
               | "dependents" ["*"] "(" ref ")"
               | "reaches" "(" ref ")"
               | "path" "(" ref "," ref ")"
               | "show" "(" ref ")"
               | "tagged" "(" StringLit ")"
               | "owner" "(" StringLit ")"
               | "unknown" [ "in" ("module"|"service"|"workflow") "(" ref ")" ]
filter       ::= "in" "module"  "(" ref ")"
               | "in" "service" "(" ref ")"
               | "owned_by" "(" StringLit ")"
               | "kind" "(" KindName ")"
KindName     ::= "module" | "service" | "workflow" | "step" | "state"
               | "event" | "type" | "error" | "function" | "external"
```

Filter and scope arguments are dotted refs; query string literals follow §15 `StringLit` with D-045b escapes (D-051). Semantics per §6.4 and D-052: scope membership is qname-prefix containment; `owner`/`owned_by` match the effective owner (declared, or for State/Event the nearest enclosing Module's); `tagged` is empty in Phase 1; results exclude the query argument, dedupe by qname keeping the shortest chain, and sort by qname. Filters intersect the result node set (D-023); `show` and `path` take none (D-052f). Bare `unknown` returns every unknown in the project. Renames from v0.1: `depends_on(X)` → `dependents(X)` (D-021); `show` and `reads` are new.

### 10.2 `show(X)` (D-022)

Prints the node card: qname, kind, origin, location, every intent clause verbatim, then in/out edges grouped by kind, each line carrying layer + status/confidence, then any findings attached to the node (per `Graph.attributions`, D-055). This is the canonical onboarding query; it MUST work for derived-only nodes (card simply shows "no declared intent").

### 10.3 Human output (normative shape)

One result per line: `qname  kind  file:line  [via: edge-kind chain]  [status/confidence]`. Header line states the query and result count. Transitive results show the shortest witnessing chain; multi-hop chains carry one status/confidence label per hop, in chain order (D-054c). The `unknown` query prints each unknown string on an indented line under its node (D-052d). Color when TTY; never color in `--json` or piped output.

### 10.4 `--json` schema (D-025)

```json
{
  "lore_version": "0.2.0",
  "query": "affects*(Payment.ledger)",
  "results": [
    {
      "qname": "PaymentService.charge",
      "kind": "Function",
      "location": {"file": "src/payments/service.py", "line": 41},
      "via": [
        {"from": "PaymentService.charge", "to": "Payment.ledger",
         "edge": "Affects", "layer": "Declared", "status": "Verified"}
      ]
    }
  ],
  "unresolved": [],
  "stats": {"nodes_visited": 124, "elapsed_ms": 3}
}
```
Field names are normative. `show` returns the node card under `"node"` plus `"edges_in"` / `"edges_out"` arrays of the same edge shape. `via` is the shortest witnessing chain in causal edge order; the event hop contributes its `Emits` and `Handles` edges (D-054). `unresolved` carries the dotted refs of the graph's `E0306` findings, deduplicated and sorted (D-047, D-053). Results of the `unknown` query form additionally carry an `"unknown"` array of the node's unknown strings (D-052d). `path --all` yields one result per witnessed path (D-054d).

### 10.5 Exit codes (all commands, D-025)

`0` success / no error-severity findings; `1` error-severity findings; `2` usage or input parse error; `3` internal error (panic boundary). For `lore ask`: an unparseable query, an argument naming no node (message names the nearest existing qname), a wrong-kind filter argument, or `--all` without `--max-len` is exit 2; graph findings never fail `ask` (D-053).

### 10.6 Engine API shape

The engine exposes two primitives: `select(predicate) -> NodeSet` and `traverse(start, direction, edge_kinds, transitive) -> NodeSet-with-witness-paths`. Every query form in §10.1 MUST be expressible as a composition of these two; new query forms are sugar, never new engine code paths. *(This keeps the door open for full composition later without committing to it now -- D-023.)*

### 10.7 Performance contract

The graph is held in memory: `HashMap<QName, Node>` plus forward and reverse adjacency `HashMap<QName, Vec<Edge>>` (both directions stored -- queries read both). Single-hop queries are O(degree); transitive queries are BFS, O(V+E). Target: any query over a 5,000-node graph answers in <50 ms on commodity hardware. No persistence layer in v1; the graph is rebuilt per invocation, with a content-hash file cache of per-file extraction facts (`.lore-cache/`, safe to delete; facts, not serialized trees -- D-064).

---

## 11. Project Manifest: lore.toml

Located at the project root; discovered by walking up from CWD. All keys shown with defaults:

```toml
[project]
name = "myproject"
languages = ["python", "typescript"]   # scanner+binder+derive activated per language
roots = ["src"]                        # import-resolution roots (§8.2)

[modules]                              # path glob -> module name (D-015)
"src/payments/**" = "Payment"
"src/users/**"    = "User"

[policy]
unknown            = "warn"            # "warn" | "error"   (D-012)
stale              = "warn"            # "warn" | "error"   (D-018)
undeclared_effects = "off"             # "off" | "warn"     (D-019)

[lint]                                 # per-finding severity overrides
# "W0206" = "off"
```

Unknown keys: `E0401`. A known key with an invalid value: `E0403` (D-043). Missing manifest: every command except `lore init` fails with `E0402` and the suggestion to run `lore init`. `[lint]` keys MUST be W-codes from the §18 registry (`E0401` otherwise -- E findings can never be silenced) and values `"warn"` or `"off"` (`E0403` otherwise); `"off"` suppresses the code from lint output and the exit-code computation, including strict-promoted instances, while `ask`/`show` stay unfiltered (D-056).

---

## 12. CLI

Single binary `lore` (crate `lore_cli`).

| Command | Does | Milestone |
|---|---|---|
| `lore init` | Write a starter `lore.toml`; detect languages; propose `[modules]` globs from directory names | T1 |
| `lore scan [--json]` | Scanner+binder only: list every annotation block, its subject, qname, kind | T1 |
| `lore ask "<query>" [--json] [--all --max-len N]` | §10 | T4 |
| `lore lint [--json] [--no-stale]` | Resolution checks, required intent, applicability, depends_on surface, hygiene (`W0210`-`W0212`), reconciliation, staleness; exit per §10.5 | T3 (structural) → T7 (full) |
| `lore stats [--json]` | Coverage: nodes by kind/origin, % declared intent per kind, claims by status (T7), unresolved_calls + ambiguous_derived_names counts (D-065) | T6 (counts) → T7 (claim statuses) |
| `lore history <qname> [--json]` | §9.3 (D-059) | T5 |
| `lore graph --dot [--focus <qname> --depth N]` | Graphviz export (D-038) | T8 |
| `lore mcp` | MCP server (stdio): tools `lore_ask`, `lore_show`, `lore_lint`, `lore_history` mapping 1:1 to the JSON outputs (D-037) | T9 |
| `lorec build / run` | Phase 2 compiler/VM driver | L1+ |

Global flags: `--manifest <path>`, `--quiet`, `--no-color`.

---

## 13. Shared Data Contracts

These are the AST contract (guideline G-3). Changing a field here is a breaking change for every downstream crate; trace all uses first.

```rust
// ---- lore_intent ----
pub struct Spanned<T> { pub value: T, pub span: Span }
pub struct Span { pub file: PathBuf, pub line: u32, pub col: u32, pub end_line: u32, pub end_col: u32 }

pub struct Intent {
    pub purpose:    Option<Spanned<String>>,
    pub owner:      Option<Spanned<String>>,
    pub because:    Vec<Spanned<String>>,
    pub unknown:    Vec<Spanned<String>>,
    pub assumes:    Vec<Spanned<String>>,
    pub affects:    Vec<Spanned<Ref>>,
    pub reads:      Vec<Spanned<Ref>>,
    pub triggers:   Vec<Spanned<Ref>>,
    pub emits:      Vec<Spanned<Ref>>,
    pub on:         Vec<Spanned<Ref>>,
    pub depends_on: Vec<Spanned<Ref>>,
    pub route:      Option<Spanned<Route>>,
    pub enforcement: Option<Spanned<Enforcement>>,
}
pub struct Ref { pub segments: Vec<String> }      // unresolved here
pub enum Enforcement { Strict, Warn }
pub struct Route { pub method: Option<HttpMethod>, pub path: String }

pub struct IntentNode {
    pub qname:  QName,            // newtype over Vec<String>
    pub kind:   Kind,
    pub origin: Origin,           // Declared | Derived | Both
    pub intent: Intent,           // empty for derived-only nodes
    pub loc:    Span,
}
pub enum Kind { Module, Service, Workflow, Step, State, Event, Type, Error, Function, External }

pub enum Severity { Error, Warning }                 // derived from the code letter (D-040)
pub struct Finding { pub code: &'static str, pub severity: Severity, pub span: Span, pub message: String }

// Language pack as pure data (§8.6, D-070/D-071). lore_cli parses and
// validates a pack, then hands the PackSpec to lore_annotations / lore_derive
// alongside the tree-sitter grammar handle as a *separate* argument -- so
// lore_intent never depends on tree-sitter (D-070d). bind_scm / derive_scm
// carry the query source text; the generic adapter compiles them at
// activation (E0411 on a compile failure or unknown capture name).
pub struct PackSpec {
    pub name: String,                    // == pack dir name, == lore.toml language id
    pub format: u32,                     // §8.6.1; this spec defines version 1
    pub tier: Tier,                      // scan | bind | derive (§8.6.2)
    pub grammar_id: Option<String>,      // statically linked grammar id (bind+); None at scan
    pub extensions: Vec<String>,         // claimed file extensions, e.g. [".go"]
    pub comment_token: String,           // line-comment token (§7.1)
    pub wrappers: Vec<String>,           // descend-wrapper node types (§7.3, D-042)
    pub sibling_skips: Vec<String>,      // preceding-sibling skips (D-050c)
    pub mutator_methods: Vec<String>,    // §8.3 receiver mutators
    pub mutator_free_functions: Vec<String>, // §8.3 first-arg mutators (Go delete)
    pub imports: Vec<ImportStrategy>,    // ordered; first resolution wins (D-071)
    pub bind_scm: Option<String>,        // declaration query source (tier bind+)
    pub derive_scm: Option<String>,      // call/import/touch query source (tier derive)
}
pub enum Tier { Scan, Bind, Derive }
pub enum ImportStrategy {                // §8.6.1, D-071 built-in strategy library
    Relative       { extensions: Vec<String>, index_files: Vec<String> },
    RootRelative   { separator: String, extensions: Vec<String>, init_files: Vec<String> },
    PackageDir     { extensions: Vec<String> },
    ManifestPrefix { manifest_file: String, prefix_key: String },
    Custom         { name: String },     // selects a registered lore_derive impl; needs a D-entry
}

// ---- lore_graph ----
pub enum EdgeKind { Affects, Reads, Triggers, Emits, Handles, DependsOn, Contains, Sequence, Calls }
pub enum Layer { Declared, Derived }
pub enum ClaimStatus { Verified, Unverified, Contradicted, Unverifiable }
pub enum Confidence { Exact, Resolved, Heuristic }

pub struct Edge {
    pub from: QName, pub to: QName, pub kind: EdgeKind,
    pub layer: Layer, pub loc: Span,
    pub status: Option<ClaimStatus>,     // Some iff layer == Declared (Affects/Reads/Triggers/Emits/Handles/DependsOn)
    pub confidence: Option<Confidence>,  // Some iff layer == Derived
}

pub struct Graph {
    pub nodes: HashMap<QName, IntentNode>,
    pub out:   HashMap<QName, Vec<Edge>>,
    pub inc:   HashMap<QName, Vec<Edge>>,
    pub findings: Vec<Finding>,          // lore_intent::Finding -- E/W codes with spans (§18, D-040)
    pub attributions: HashMap<QName, Vec<usize>>, // node -> indices into findings (D-049 attribution, public per D-055)
}

// CODEOWNERS data passed into lore_graph::build by the CLI (D-058) --
// the graph consumes it as data, never reading the filesystem itself.
pub struct Codeowners { pub file: PathBuf, pub rules: Vec<CodeownersRule> }
pub struct CodeownersRule { pub pattern: String, pub owners: Vec<String> }

// The derived layer as data, passed into lore_graph::build by the CLI from
// lore_derive's output (the graph never depends on the derive crate, §13).
pub struct DerivedLayer {
    pub nodes: Vec<IntentNode>,          // origin Derived, empty intent (§8.1)
    pub edges: Vec<Edge>,                // layer Derived: Calls/Affects/Reads with confidence
    pub scope: HashSet<PathBuf>,         // derivation scope (D-061): §9.1's in-scope test
}

// Reconciliation inputs (§9, D-066/D-068): source text and git metadata as
// data, CLI-supplied -- the graph never reads the filesystem or runs git.
pub struct ReconcileInput {
    pub sources: HashMap<PathBuf, String>,        // file -> text, for the §9.1 occurrence test
    pub host_identifiers: HashMap<QName, String>, // declared nodes' bound host identifiers (binder extraction)
    pub staleness: Option<Vec<StalenessRecord>>,  // None = check skipped (no git / --no-stale / non-lint command)
}

pub struct StalenessRecord {                      // one per annotation block with a subject span (D-068)
    pub qname: QName,
    pub span: Span,                               // the block's span: where W0301 points
    pub t_block: i64,                             // max committer-time over block lines, unix seconds
    pub t_subject: i64,                           // max committer-time over subject-span lines
    pub t_block_iso: String,                      // ISO-strict renderings for the message
    pub t_subject_iso: String,
    pub subject_commit: String,                   // hash of the subject line attaining the max
}
```

Dependency direction (no crate reaches backwards):
```
Phase 1:  cli -> graph -> intent
          cli -> annotations -> intent
          cli -> derive -> intent
          graph consumes outputs of annotations + derive (data, not deps)
Phase 2:  cli -> semantic -> parser -> lexer
          semantic -> graph ;  cli -> bytecode -> parser ;  cli -> vm -> bytecode
```

Crate layout: `crates/{lore_intent, lore_annotations, lore_derive, lore_graph, lore_cli, lore_lexer, lore_parser, lore_semantic, lore_bytecode, lore_vm}`.

---

## 14. Phase 2: The Lore Language

Everything in §§3--6 applies unchanged; this section adds what only the language has. File extension `.lore`; compiler `lorec`.

### 14.1 Types

Primitives (D-029): `Text`, `Integer` (i64), `Float` (f64), `Bool`, `Decimal` (arbitrary precision), `UUID`, `Unit`.
Builtin generics: `Result<T, E_union>`, `Option<T>`, `List<T>`, `Map<K, V>`.
Stdlib (not primitives): `std.Email`, constructed via `std.Email.parse(t: Text) -> Result<std.Email, std.InvalidEmail>`.

**Semantic tags:** `def type UserId = UUID tagged "user-identity"`. Tag comparison is exact string equality, case-sensitive, no hierarchy. Mixing differently-tagged values without explicit conversion is `E0503`. Tags are erased at runtime, retained as node attributes (`tagged("...")` query).

**Records / variants / sealed:** as v0.1 -- newline-separated fields; `|`-prefixed variants with optional named fields; exhaustive matching; `sealed` types constructible only inside the owning module (`E0504`), forcing construction through a `def new`.

**Record update (D-028):** `user with { verified: true }`. `|` is no longer a binary operator anywhere.

**Error unions (D-027):** the error position of `Result` is a union of `def error` refs: `Result<Receipt, InsufficientFunds | GatewayError>`. `raise X {...}` requires `X ∈` the enclosing function's union (`E0505`). `expr?` requires `members(err(expr)) ⊆ members(err(fn))` (`E0506`), unwraps `Ok`, early-returns `Err` unchanged. No implicit conversion. `match` on an error value is exhaustive over union members.

### 14.2 State (D-030)

```
def state ledger: List<LedgerEntry>
  purpose: "Append-only record of every money movement"
```
State lives only in modules. Access is via qualified name + the builtin state API -- **the only mutation surface in the language**:

| State type | Write ops | Read ops |
|---|---|---|
| `List<T>` | `.append(v) -> Unit` | `.items() -> List<T>`, `.len() -> Integer` |
| `Map<K,V>` | `.put(k,v) -> Unit`, `.remove(k) -> Option<V>` | `.get(k) -> Option<V>`, `.entries() -> List<(K,V)>` |

Under `enforcement: strict` on the state's owning module: a write op requires the state in the caller's `affects` (`E0507`); a read op requires it in `reads` or `affects` (`E0508`). This check is what makes Phase 2 grounding total -- every effect claim is verified against actual call sites, so every `Affects`/`Reads` claim in `.lore` code is `Verified` or a compile error.

### 14.3 Externals (D-009, D-033)

```
def external Gateway
  purpose: "Card-network payment gateway"
  owner: "payments-team"
  assumes: "Gateway is idempotent per provided idempotency key"
{
  def charge(method: PaymentMethod, amount: Price, key: UUID)
    -> Result<ReceiptId, GatewayError | GatewayTimeout>
  def refund(receipt: ReceiptId) -> Result<Unit, GatewayError>
}
```
Signatures only, no bodies. Calls type-check normally. Binding to implementations is a runtime concern (§17.4).

### 14.4 Services, events, workflows

Unchanged from v0.1 semantics, with: service intent requires `purpose`, `owner`, base `route`; per-function `route` marks handlers, routeless functions are private helpers; `def event` requires `purpose`, payload is a field list; workflows are top-level, steps ordered, each step declares ≥1 of `triggers/emits/on`.

---

## 15. Phase 2: Formal Grammar

Lexing: `//` line comments ignored; NEWLINE tokens emitted and significant as separators inside `{}` bodies and field/variant lists, suppressed inside `()` `<>` and after a binary operator or `,` (D-034).

```ebnf
program        ::= ( module | service | workflow | external )*

module         ::= "def" "module" Ident module_intent "{" module_body "}"
module_body    ::= ( type_def | state_def | event_def | error_def | func_def | module )*
service        ::= "def" "service" Ident service_intent "{" func_def* "}"
workflow       ::= "def" "workflow" Ident workflow_intent "{" step+ "}"
step           ::= "step" Ident "{" intent_clause* "}"
external       ::= "def" "external" Ident external_intent "{" extern_sig* "}"
extern_sig     ::= "def" Ident "(" param_list? ")" "->" type_expr intent_clause*

module_intent   ::= purpose_clause owner_clause intent_clause*
service_intent  ::= purpose_clause owner_clause route_clause intent_clause*
workflow_intent ::= purpose_clause owner_clause intent_clause*
external_intent ::= purpose_clause owner_clause intent_clause*
intent_block    ::= intent_clause*

intent_clause  ::= purpose_clause | owner_clause | because_clause | unknown_clause
                 | assumes_clause | affects_clause | reads_clause | triggers_clause
                 | emits_clause | on_clause | depends_clause | route_clause
                 | enforcement_clause
(* The grammar permits any clause anywhere; the applicability matrix (§3.2)
   is enforced by semantic analysis with E0203/E0205. *)

purpose_clause     ::= "purpose" ":" StringLit
owner_clause       ::= "owner" ":" StringLit
because_clause     ::= "because" ":" StringLit
unknown_clause     ::= "unknown" ":" StringLit
assumes_clause     ::= "assumes" ":" StringLit
affects_clause     ::= "affects" ":" ref_list
reads_clause       ::= "reads" ":" ref_list
triggers_clause    ::= "triggers" ":" ref_list
emits_clause       ::= "emits" ":" ref_list
on_clause          ::= "on" ":" ref_list
depends_clause     ::= "depends_on" ":" ref_list
route_clause       ::= "route" ":" http_method StringLit | "route" ":" StringLit
http_method        ::= "GET" | "POST" | "PUT" | "DELETE" | "PATCH"
enforcement_clause ::= "enforcement" ":" ( "strict" | "warn" )
ref_list           ::= ref ( "," ref )*
ref                ::= Ident ( "." Ident )*

type_def       ::= "def" "type" Ident ( "sealed" )?
                   ( "=" type_expr | "{" field_list "}" | "{" variant_list "}" )
state_def      ::= "def" "state" Ident ":" type_expr intent_block
event_def      ::= "def" "event" Ident purpose_clause intent_clause* "{" field_list "}"
error_def      ::= "def" "error" Ident "{" error_field_list "}"
error_field_list ::= ( field | because_field ) ( NEWLINE ( field | because_field ) )*
because_field  ::= "because" ":" StringLit

field_list     ::= field ( NEWLINE field )*
field          ::= Ident ":" type_expr
variant_list   ::= variant ( NEWLINE variant )*
variant        ::= "|" Ident ( "(" field_list ")" )?

type_expr      ::= "Result" "<" type_expr "," error_union ">"
                 | "Option" "<" type_expr ">" | "List" "<" type_expr ">"
                 | "Map" "<" type_expr "," type_expr ">"
                 | ref ( "tagged" StringLit )?
error_union    ::= ref ( "|" ref )*

func_def       ::= "def" Ident "(" param_list? ")" "->" type_expr intent_block block
param_list     ::= param ( "," param )* ;  param ::= Ident ":" type_expr

block          ::= "{" expr_seq "}"
expr_seq       ::= seq_item ( NEWLINE seq_item )*
seq_item       ::= let_binding | expr        (* final item MUST be expr: E0509 *)
let_binding    ::= "let" Ident "=" expr

expr           ::= match_expr | if_expr | raise_expr | emit_expr | with_expr
emit_expr      ::= "emit" ref "{" field_init_list? "}"
(* emit has type Unit; legal only when the event is declared in the enclosing
   function's or step's emits clause: E0512. *)
with_expr      ::= or_expr ( "with" "{" field_init_list "}" )?
or_expr        ::= and_expr ( "or" and_expr )*
and_expr       ::= cmp_expr ( "and" cmp_expr )*
cmp_expr       ::= add_expr ( ( "==" | "!=" | "<" | ">" | "<=" | ">=" ) add_expr )?
add_expr       ::= mul_expr ( ( "+" | "-" ) mul_expr )*
mul_expr       ::= unary_expr ( ( "*" | "/" ) unary_expr )*
unary_expr     ::= ( "not" | "-" )? postfix_expr
postfix_expr   ::= primary_expr ( "(" arg_list? ")" | "." Ident | "?" )*
primary_expr   ::= ref | Literal | "(" expr ")" | record_construct
record_construct ::= ref "{" field_init_list "}"
field_init_list  ::= field_init ( ( "," | NEWLINE ) field_init )*
field_init       ::= Ident ":" expr

match_expr     ::= "match" expr "{" match_arm+ "}"
match_arm      ::= pattern "->" ( expr | block ) NEWLINE?
pattern        ::= "_" | ref ( "(" pattern_bind ( "," pattern_bind )* ")" )?
pattern_bind   ::= Ident | "_"
raise_expr     ::= "raise" ref "{" field_init_list? "}"
if_expr        ::= "if" expr block ( "else" ( block | if_expr ) )?

Literal        ::= StringLit | Integer | Float | "true" | "false"
StringLit      ::= '"' ( [^"\\] | "\\" . )* '"'
Ident          ::= [a-zA-Z_][a-zA-Z0-9_]*
```

Resolved ambiguities: `|` appears only in `variant` and `error_union` (D-028); `raise` has bottom type `Never` and unifies with any branch; `if` without `else` has type `Unit` and is legal only as a non-final seq_item (`E0510` if final and the function's return type is not `Unit`).

---

## 16. Phase 2: Semantic Rules

Checked by `lore_semantic`, which also produces `IntentNode`s and feeds `lore_graph` (identical graph code as Phase 1).

1. **Applicability matrix** (§3.2) → `E0201/E0203/E0204/E0205`.
2. **Resolution + kind checks** (§6.3) → `E0306/E0307`; `depends_on` surface → `E0304/W0206`.
3. **Type checking:** structural inference within function bodies; declared signatures are the truth at boundaries. Tag mismatch `E0503`; sealed construction `E0504`.
4. **Error unions:** `raise` membership `E0505`; `?` subset rule `E0506`.
5. **State permissions** under strict: write→`affects` `E0507`; read→`reads` `E0508`. **Emit permission:** `emit E {...}` requires `E` in the enclosing function's/step's `emits` clause (`E0512`, all enforcement levels -- an undeclared emit is always an error because it is a silent graph hole).
6. **Exhaustive match** over variants and error unions → `E0511` (missing arms named in the message).
7. **Effect derivation:** the compiler derives `Calls`, `Affects`, `Reads` edges from bodies and `Emits`/`Handles` from `emit`/handler registration -- so in Phase 2 every claim kind is reconciled and `Unverifiable` disappears for `.lore` code.

---

## 17. Phase 2: Execution Model

### 17.1 VM shape

Stack-based VM; call frames hold locals + return address. Errors are values: `raise` constructs an `Err` and returns it -- no unwinding, no exceptions. Services are VM units: `lorec run <Service>` registers routes and serves. Scaling/orchestration permanently out of scope.

### 17.2 Bytecode

`.lorec` file: magic `LOREC`, `u16 version = 1`, `u16 reserved`, then constant pool, function table, route table, state table, event table, code. Unknown version → clean rejection before execution (D-035). Intent payloads (`unknown` messages, `because` strings, state/event ids) are in the constant pool so `lore trace` can read them.

### 17.3 Instruction set (initial; revisable only at L3 via a decision entry)

`LOAD_CONST i` · `LOAD_LOCAL i` · `STORE_LOCAL i` · `CALL fref argc` · `EXTERN_CALL xref argc` · `RETURN` · `TRY_PROPAGATE` (the `?` op: if TOS is Err, RETURN it; else unwrap) · `JUMP a` / `JUMP_IF_FALSE a` · `MATCH_VARIANT tag a` · `MATCH_FAIL` · `RAISE eref fieldc` · `RECORD_CONSTRUCT tref fieldc` · `RECORD_UPDATE fieldc` · `FIELD_GET fref` · arithmetic/compare ops · `STATE_READ sref op` / `STATE_WRITE sref op` (op selects the §14.2 API method; the VM checks nothing here -- permissions were compile-time) · `AFFECTS_BEGIN sid` / `AFFECTS_END sid` · `UNKNOWN_WARN mid` · `EMIT_EVENT eid` · `ON_EVENT eid fref` · `ROUTE_REGISTER m path fref`.

### 17.4 Runtime bindings -- `lore.runtime.toml` (D-031, D-033)

```toml
[state."Payment.ledger"]
backend = "memory"            # only legal value in v1; "postgres"/"redis" reserved

[external."Gateway"]
provider = "host:gateway_v1"  # key into the VM's registered HostFn table
```
Unbound external at startup = launch error. State default is `memory` if unlisted.

### 17.5 Event delivery (D-032)

In-process, async. `EMIT_EVENT` enqueues onto a per-process dispatcher; queued events are released only when the emitting function returns `Ok` (an `Err` return discards its emissions). FIFO per event type; at-most-once; handler `Err` is logged + counted, never propagated. Single dispatcher thread in v1. `lore trace` exposes emit/handle/failure counters.

### 17.6 `lore trace`

Queries the graph against a live VM over a local socket: active `AFFECTS` regions, hit `UNKNOWN_WARN` paths with counts, route traffic, event counters. Read-only. Specified in detail at L6 (placeholder; do not build earlier).

---

## 18. Diagnostics Registry

Every diagnostic MUST state: what went wrong (plain language), where (file:line from the span), and what to do. The code registry is normative; new diagnostics take the next free number in their band.

| Band | Area |
|---|---|
| E010x/W010x | Scanner & binder (`E0102` unbound annotation, `E0103` overlapping module globs, `E0104` ambiguous assignment target, `E0105` step outside workflow, `E0106` invalid kind value, `E0107` invalid name value, `E0108` scoping block missing name, `E0109` non-scoping block missing name in a scan-tier language -- D-070) |
| E020x/W020x | Intent parsing & applicability (`E0201` missing required intent, `E0202` unknown clause, `E0203` illegal clause for kind, `E0204` empty step, `E0205` route outside service, `E0206` duplicate singular clause, `E0207` malformed clause, `W0205` intra-module triggers, `W0206` unused depends_on, `W0207` CODEOWNERS mismatch, `W0208` orphan file, `W0209` missing recommended purpose, `W0213` declared unknown -- D-057) |
| E030x/W030x | Graph, reconciliation, hygiene (`E0302/W0302` contradicted claim, `E0304` undeclared dependency use, `E0305` duplicate qname, `E0306` unresolved ref, `E0307` wrong-kind ref, `W0301` stale intent, `W0303` undeclared effect, `W0210` orphaned state, `W0211` event without handlers, `W0212` event without emitters) |
| E040x | Manifest (`E0401` unknown key, `E0402` missing manifest, `E0403` invalid manifest value) |
| E041x | Language packs, §8.6 (D-070: `E0410` invalid pack manifest, `E0411` unusable pack artifact, `E0412` unsupported pack format version, `E0413` grammar unavailable, `E0414` unknown or misconfigured import strategy, `E0415` conformance failure) |
| E05xx | Phase 2 semantics (§16) |
| E06xx | VM/runtime (reserved) |

Severity defaults are the letter (E/W); `lore.toml [lint]` may override W↔off (domains and application point per D-056) and promote per `[policy]` (`unknown` promotes `W0213`, D-057; `stale` promotes `W0301` at T7); module `enforcement: strict` promotes that module's W findings from bands 02x/03x to errors (attribution and nearest-module rule per D-049).

---

## 19. Canonical Example

### Phase 1 -- annotated Python (`src/payments/service.py`, module `Payment` via lore.toml)

```python
# @lore
# kind: state
# name: ledger
# purpose: "Append-only record of every money movement"
ledger = []

# @lore
# kind: event
# name: PaymentSettled
# purpose: "Funds have moved and the ledger has been updated"
PAYMENT_SETTLED = "payment.settled"

# @lore
# purpose: "Charge a customer"
# because: "Idempotency key is generated by the caller -- we do not deduplicate here"
# assumes: "amount is non-negative and already currency-validated"
# affects: Payment.ledger
# reads: Payment.balances
# emits: Payment.PaymentSettled
# unknown: "Behavior under concurrent charge + refund on one account is untested"
def charge(user_id, amount):
    if balances.get(user_id, 0) < amount:        # derived: Reads balances (Heuristic)
        raise InsufficientFunds(user_id, amount)
    ledger.append(entry(user_id, amount))        # derived: Affects ledger (Heuristic)
    publish(PAYMENT_SETTLED, user_id, amount)
```

`lore lint` here: `affects: Payment.ledger` → **Verified**; `reads: Payment.balances` → Verified; `emits` → Unverifiable (P1); plus `W0302` the day someone deletes the `ledger.append` line, and `W0301` the day the body changes without the block changing.

### Phase 2 -- `payment.lore` (excerpt showing every resolved construct)

```
def module Payment
  purpose: "Money movement primitives"
  owner: "payments-team"
  enforcement: strict
  depends_on: User
{
  def type Price = Decimal tagged "currency:USD"
  def type ReceiptId = UUID tagged "payment-receipt"

  def type LedgerEntry {
    account: User.UserId
    amount:  Price
  }

  def state ledger: List<LedgerEntry>
    purpose: "Append-only record of every money movement"
  def state balances: Map<User.UserId, Price>
    purpose: "Current available funds per account"

  def event PaymentSettled
    purpose: "Funds have moved and the ledger has been updated"
  {
    account: User.UserId
    amount:  Price
  }

  def error InsufficientFunds {
    account:   User.UserId
    requested: Price
    available: Price
    because:   "Balance check passed but funds moved concurrently"
  }
}

def external Gateway
  purpose: "Card-network payment gateway"
  owner: "payments-team"
  assumes: "Gateway is idempotent per provided idempotency key"
{
  def charge(account: User.UserId, amount: Payment.Price, key: UUID)
    -> Result<Payment.ReceiptId, Payment.GatewayError>
}

def service PaymentService
  purpose: "Handle all payment transactions for the platform"
  owner: "payments-team"
  route: "/payments"
  depends_on: User, Payment, Gateway
  unknown: "Behavior under concurrent charge + refund on one account is untested"
{
  def charge(userId: User.UserId, amount: Payment.Price)
    -> Result<Payment.ReceiptId, Payment.InsufficientFunds | Payment.GatewayError>
    route: POST "/charge"
    because: "Idempotency key is generated by the caller -- we do not deduplicate here"
    assumes: "amount is non-negative"
    reads:   Payment.balances
    affects: Payment.ledger, Payment.balances
    emits:   Payment.PaymentSettled
  {
    let available = match Payment.balances.get(userId) {
      Option.Some(v) -> v
      Option.None    -> Price.zero()
    }
    if available < amount {
      raise Payment.InsufficientFunds {
        account: userId, requested: amount, available: available
      }
    }
    let receipt = Gateway.charge(userId, amount, UUID.generate())?
    Payment.ledger.append(Payment.LedgerEntry { account: userId, amount: amount })
    Payment.balances.put(userId, available - amount)
    emit Payment.PaymentSettled { account: userId, amount: amount }
    receipt
  }
}
```

Under `strict`, deleting the `affects: Payment.ledger` line makes the `.append` call `E0507`; deleting the `.append` call makes the claim `Contradicted`. That symmetry -- the claim and the code policing each other -- is the language's reason to exist.

---

*End of specification. Deviations require a `lore-decisions.md` entry (guideline G-1).*
