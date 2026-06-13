# Lore Decisions Ledger

**Version:** 1.0
**Status:** Binding
**Rule:** Every decision below is final until superseded by a new entry with a higher number. Implementation MUST NOT silently deviate. To change a decision, add a new entry `D-0NN supersedes D-0MM` with rationale -- never edit an existing entry.

Format: ID | Question | Decision | Rationale | Blocks/affects.

---

## Architecture

**D-001 -- Is the intent graph declared, derived, or both?**
Decision: Both, with the derived graph as ground truth. The graph has two layers: the **declared layer** (built from `@lore` annotations / `.lore` intent blocks -- the *why*) and the **derived layer** (built by static extraction from host source -- the *what*). A **reconciliation pass** compares them and assigns every declared edge a claim status (`Verified`, `Unverified`, `Contradicted`, `Unverifiable`).
Rationale: A system of unchecked claims inherits the trust problem of comments. Grounding declared edges against derived facts is the answer to "this will drift like comments" and is the single highest-leverage capability of the platform. Phase 2 makes grounding total via the compiler; Phase 1 makes it partial via static extraction.
Affects: crate set (adds `lore_derive`), milestones T6--T8, lint semantics.

**D-002 -- Cold start: is the tool useful before annotation coverage exists?**
Decision: Yes. The derived layer builds Function/Type nodes and Call/StateTouch edges with zero annotations, so `lore ask` answers structural queries on an unannotated repo. Annotations enrich; they are not a prerequisite.
Rationale: Closed-world declared-only resolution made the value curve backloaded; derived-first deletes the adoption cliff.
Affects: query engine, T6.

**D-003 -- Phase ordering.**
Decision: Phase 1 (tool: annotations + derivation + reconciliation + queries) ships and is validated on real repositories before any Phase 2 (language) crate is started. Unchanged from prior guidelines, now with grounding milestones inside Phase 1.

## Intent clauses

**D-004 -- `version:` / `changed:` clauses.**
Decision: **Removed from the language and annotation format.** Change history is derived from VCS: `lore history <qname>` runs `git log -L` over the subject span and renders commit messages. Decision rationale belongs in commit messages (guideline G-10) and `because`.
Rationale: Hand-maintained dates duplicate git and will be wrong within a month; the prior spec contradicted its own guideline 10.

**D-005 -- `assumes:` clause.**
Decision: **Added.** `assumes: "<prose>"`, repeatable, legal on functions, workflow steps, and externals. Pure declared-layer metadata (no derived counterpart, no resolution).
Rationale: "What does this assume about its inputs?" is in the problem statement and no prior clause captured it.

**D-006 -- `reads:` clause and read edges.**
Decision: **Added.** `reads: ref, ...` resolving to `State` nodes, edge kind `Reads`. `affects` is now defined strictly as **write/mutate**; `reads` as **read without mutation**. Blast-radius queries traverse both.
Rationale: Schema-change blast radius is mostly readers; `affects`-only captured half the use case.

**D-007 -- `triggers` boundary.**
Decision: `triggers` declares **cross-module synchronous calls only**. A `triggers` ref whose target is in the same module is lint warning `W0205` (redundant -- intra-module call edges are derived). In Phase 2 the compiler derives all call edges; declared `triggers` are reconciled against them like any claim.
Rationale: Without a boundary, `triggers` either explodes into declaring every call (ceremony) or is undefined. Cross-module is where intent matters and where derivation is least reliable.

**D-008 -- `depends_on` semantics.**
Decision: `depends_on` is an **assertion of the dependency surface**, not load-bearing data. Rules: (a) any qualified ref to module M from construct C requires M (or M's owning module) in C's effective `depends_on` -- violation is `E0304`; (b) a declared dependency with no qualified ref using it is `W0206` (unused dependency). Targets: `Module`, `Service`, `External`.
Rationale: It is derivable, so its value is making dependency growth deliberate and lintable -- say so and enforce both directions.

**D-009 -- External dependencies (`Database.postgres` problem).**
Decision: New node kind **`External`** with construct `def external` (Phase 2) / `kind: external` (Phase 1). An external declares a name, required `purpose`, required `owner`, and (Phase 2) a set of body-less typed function signatures. `depends_on` may target externals. The prior `Database.postgres`-style magic names are removed; examples rewritten.
Rationale: The prior spec's own flagship example violated its resolution rules.

**D-010 -- `owner` vs CODEOWNERS.**
Decision: `owner` stays a first-class clause. `lore lint` gains check `W0207`: if a `CODEOWNERS` file exists and maps the subject's path to a different owner than declared, warn. No automatic import in v1.
Rationale: Keep one declared source inside the graph, but never let it silently contradict the org's existing source of truth.

**D-011 -- Enforcement levels.**
Decision: Two levels: `strict` and `warn`. `doc` removed. Default `warn`. In Phase 1, `strict` means lint findings in that module are errors (non-zero exit); in Phase 2, compile errors.

**D-012 -- `unknown` promotion.**
Decision: Project policy in `lore.toml`: `[policy] unknown = "warn" | "error"` (default `warn`). No per-annotation severity field in v1.

## Phase 1 mechanics

**D-013 -- Binder implementation.**
Decision: **tree-sitter from T1.** The scanner needs only comment tokens; the binder and the derived layer both use tree-sitter grammars. Per-language declaration-node tables are normative in the spec (§7.4). `name:` overrides the extracted identifier; an annotation that cannot be bound to a declaration node is error `E0102`.
Rationale: "One-line config per language" was true only for the scanner; regex binding is unmaintainable; tree-sitter is required for derivation anyway (D-001), so the cost is already sunk.

**D-014 -- Supported host languages, v1 order.**
Decision: Python, TypeScript (covers JS) first; then Go, Java, Rust. Scanner+binder for all five by T8; derived layer for Python+TypeScript at T6, the rest at T8.

**D-015 -- Module scoping in Phase 1.**
Decision: Two mechanisms, with precedence: (1) `lore.toml [modules]` maps path globs to module names (directory scoping); (2) a top-of-file `kind: module` block scopes that single file and **overrides** the toml mapping for that file. A file matched by neither has no module; its annotated subjects get qname `_orphan.<name>` and lint finding `W0208`. Two different module names claiming one file via mechanism (1) (overlapping globs) is `E0103`.
Rationale: Cross-file membership needed a precise rule; "placed on a package or class" was undefined.

**D-016 -- Duplicate qualified names.**
Decision: Error `E0305`. Two nodes may not share a qname. No merging.

**D-017 -- Reference resolution scope.**
Decision: Declared refs resolve against the union of declared **and derived** nodes. Resolving to a derived-only node is legal and marks the edge `Unverified-declared-target` is not a thing -- the edge is normal; the target simply lacks declared intent (surfaced by `lore stats` coverage). Resolving to nothing is `E0306` (unresolved ref). Resolving to a wrong-kind node is `E0307` with the kind table in §6.3.
Rationale: Closed-world-over-annotations (old rule) blocked cold start; the derived layer provides the open world safely.

**D-018 -- Staleness detection.**
Decision: `lore lint` includes `stale-intent` (`W0301`): for each annotation block, compare `max(git commit time of subject-span lines)` vs `max(git commit time of block lines)` via `git blame --line-porcelain`; if the subject changed strictly later, warn. Skipped (with notice) outside a git work tree. Policy-promotable to error.

**D-019 -- Reconciliation statuses and findings.**
Decision: For each declared `Affects`/`Reads`/`Triggers` edge f→t:
- `Verified` -- a matching derived edge exists.
- `Contradicted` (`E0302` under `strict`, `W0302` under `warn`) -- t's bound host symbol is in derivation scope AND f's subject span contains **zero** occurrences of that symbol.
- `Unverified` -- symbol occurs in f's span but no classified derived edge (heuristics could not classify).
- `Unverifiable` -- t is outside derivation scope (unsupported language, external, cross-repo).
Undeclared derived effects (derived `Affects` with no declaration) are `W0303`, default **off** (`[policy] undeclared_effects = "off"`), because they punish low coverage.

**D-020 -- Derived-edge confidence.**
Decision: Every derived edge carries `confidence: Exact | Resolved | Heuristic` (definitions §8). All outputs that print derived edges MUST print confidence. Heuristic edges never produce `Contradicted` on their own absence -- contradiction requires the symbol-occurrence test (D-019), which is independent of edge classification.
Rationale: Never present a guess as a fact; the tool's entire value is trust.

## Query language

**D-021 -- `depends_on(X)` query renamed.**
Decision: `dependents(X)` returns incoming `DependsOn` edges (who depends on X); `depends(X)` returns outgoing. The old name answered the opposite of what it said.

**D-022 -- `show(X)` query added.**
Decision: `lore ask show(X)` prints the node card: kind, location, full intent (purpose/owner/because/assumes/unknown), all in/out edges grouped by kind with claim status and confidence. This is the onboarding query.

**D-023 -- Query filters.**
Decision: Any node-set query accepts trailing filters: `in module(M)`, `in service(S)`, `owned_by("team")`, `kind(K)`. Filters intersect. Full composition (boolean algebra, joins) is explicitly deferred; the engine's internal API (§10.6) is predicate-and-traversal shaped so filters are sugar, not special cases.

**D-024 -- Traversal matrix.**
Decision: Normative table in spec §6.4 defines, per query, exactly which edge kinds are traversed and in which direction. The prior `affects*` contradiction (triggers-only vs event-hop) is resolved: **transitive queries traverse `Triggers` and the `Emits→Event→Handles` composite hop**; `reaches`/`path` additionally traverse `Contains`, `DependsOn`, `Affects`, `Reads`.

**D-025 -- Output and exit codes.**
Decision: All commands support `--json` with the schema in §10.7. Exit codes: `0` success/no findings, `1` findings at error severity, `2` usage or input parse error, `3` internal error. Human output format is specified, not improvised.

**D-026 -- Hygiene surfaced in lint, stats in a command.**
Decision: Orphaned state (no in `Affects`/`Reads`), events with emitters but no handlers (and inverse), unowned required-intent nodes → lint findings (`W0210`--`W0212`). Coverage metrics → `lore stats`.

## Phase 2 language

**D-027 -- Error propagation.**
Decision: **Error unions + `?` operator.** `Result<T, E1 | E2 | ...>` where each Eᵢ is a `def error` ref. `raise X {...}` is legal iff X is a member of the enclosing function's error union. `expr?` on `Result<T, E_expr>` unwraps `Ok` or early-returns `Err`; legal iff `members(E_expr) ⊆ members(E_fn)`. No implicit conversions, no `From` machinery. Match over an error value is exhaustive over union members.
Rationale: Precise, implementable (Zig-style error sets), zero hidden control flow beyond `?`.

**D-028 -- Record update operator.**
Decision: `with` keyword replaces `|`: `user with { verified: true }`. `|` is removed from the binary-operator set; it remains only as the variant prefix.
Rationale: The prior grammar was ambiguous (`|` as binary op, variant prefix, and update operator).

**D-029 -- Primitive types.**
Decision: Primitives: `Text`, `Integer`, `Float`, `Bool`, `Decimal`, `UUID`, `Unit`. `Email` demoted to the standard library (`std.Email = Text tagged "email"` with a validating constructor). Builtin generics gain **`Map<K, V>`** (required by the state API, D-030).

**D-030 -- State access and mutation.**
Decision: `def state` values are accessed by qualified name and mutated ONLY through a builtin state API -- the sole mutation surface in the language:
- `List<T>` state: `.append(v) -> Unit` (write), `.items() -> List<T>` (read snapshot), `.len() -> Integer` (read).
- `Map<K,V>` state: `.put(k, v) -> Unit` (write), `.remove(k) -> Option<V>` (write), `.get(k) -> Option<V>` (read), `.entries() -> List<(K,V)>` (read).
Under `strict`, a write op requires the state in the function's `affects`, a read op requires it in `reads` (or `affects` -- write implies read permission); violations are compile errors. This is what makes Phase 2 grounding total: effect declarations are checked against actual state-API call sites.

**D-031 -- State backing at runtime.**
Decision: v1: VM-managed in-memory, per-service process, lifetime = process. A runtime manifest `lore.runtime.toml` maps state qnames to backends; v1 accepts only `backend = "memory"`; the key space reserves `postgres`/`redis`. Persistence is post-L7 work and never blocks the graph.

**D-032 -- Event delivery semantics.**
Decision: v1: in-process, asynchronous. `EMIT_EVENT` enqueues; handlers run **after the emitting function returns `Ok`** (emit inside a function that ultimately returns `Err` is discarded). FIFO per event type. At-most-once per process (no persistence). A handler returning `Err` is logged and counted (visible to `lore trace`), never propagated to the emitter. Handlers run sequentially on a dedicated dispatcher thread in v1.

**D-033 -- Externals at runtime.**
Decision: `def external` signatures are an FFI boundary. The VM holds a host-function table; `lore.runtime.toml` maps `external qname -> host plugin` (v1: Rust functions registered at VM startup via a `HostFn` trait; dynamic loading deferred). Calling an unbound external at startup is a launch error, not a runtime panic.

**D-034 -- Newline significance.**
Decision: The lexer emits `NEWLINE` tokens. Newlines are item separators inside `{}` bodies, field lists, and variant lists; they are insignificant inside `()`, `<>`, and after a binary operator or `,`. One rule, stated once, applied by the lexer (token suppression), not ad-hoc in the parser.

**D-035 -- Bytecode versioning.**
Decision: `.lorec` header: magic bytes `LOREC`, `u16` format version (starts at 1), `u16` reserved. VM rejects unknown versions with a clear error before executing anything.

**D-036 -- `db.find(User.User, id)` style.**
Decision: Removed. First-class type values are out of v1. Canonical examples use the state API (D-030) and externals (D-033). The reflective style may return post-L7 as a generic-function design, via a new decision.

## Product

**D-037 -- MCP integration.**
Decision: `lore mcp` serves the query engine over the Model Context Protocol (stdio transport), exposing tools `lore_ask`, `lore_show`, `lore_lint`, `lore_history`. Lore never *requires* an LLM (the "not an AI tool" stance stands); it *feeds* them. Milestone T9.
Rationale: The intent graph is exactly the context coding agents lack; this is the strongest adoption wedge and costs little once `--json` exists.

**D-038 -- Graph export.**
Decision: `lore graph --dot [--focus <qname> --depth N]` emits Graphviz DOT. Milestone T8.

**D-039 -- Thesis validation gate (Phase 1 → Phase 2).**
Decision: Phase 2 starts only after T10 exit criteria pass (roadmap §T10): on a real ≥20k-LOC repo, the five canonical onboarding questions are answered correctly by `lore ask`, and seeded-drift detection catches ≥8/10 planted lies. Numbers are in the roadmap and are the gate, not a vibe.

## T1 implementation clarifications

**D-040 -- Where does `Finding` live, and what is its shape?**
Decision: `Finding` is defined in `lore_intent` (the shared base crate) so that `lore_annotations`, `lore_derive`, and `lore_graph` can all produce findings without violating the dependency direction (§13). Shape: `{ code: &'static str, severity: Severity, span: Span, message: String }` with `Severity ::= Error | Warning` derived from the code's letter (E→Error, W→Warning). Spec §13 updated to define it.
Rationale: §13 referenced `Vec<Finding>` without defining the type or its crate; scanner/binder diagnostics (E010x) exist from T1, before `lore_graph` does.
Affects: §13, every crate that reports diagnostics.

**D-041 -- Diagnostics for malformed binding lines and scoping blocks.**
Decision: Three new scanner-band codes: `E0106` invalid or duplicate `kind:` value; `E0107` invalid or duplicate `name:` value (a name must match `Ident ("." Ident)*`); `E0108` scoping block (kind `module`/`service`/`workflow` used for file scoping) missing the required `name:` field. Added to §18 band E010x.
Rationale: §7.2's grammar admits only the ten kind keywords and the ref-shaped name, but no diagnostic existed for violations; a scoping block has no subject declaration to extract an identifier from, so its name cannot be inferred.
Affects: §7.2, §7.5, §18.

**D-042 -- Binder mechanics: wrappers, identifier fields, scoping-block exemption.**
Decision: (a) Python's wrapper/skip set gains `expression_statement` (tree-sitter wraps module/class-level `assignment` in it). (b) The subject identifier comes from these tree-sitter fields: Python `function_definition.name`, `class_definition.name`, `assignment.left` (must be a plain `identifier`, else `E0104` unless `name:` is given); TS/JS `function_declaration.name`, `class_declaration.name`, `method_definition.name`, `interface_declaration.name`, `type_alias_declaration.name`, `enum_declaration.name`; `lexical_declaration`/`variable_declaration` use their single `variable_declarator`'s `name` -- multiple declarators require `name:` (`E0104` generalizes beyond Python assignments). (c) Blocks of kind `module`, `service`, or `workflow` are **scoping blocks**: they bind to no declaration and are exempt from `E0102`; the first such block in a file scopes the file (§7.5 rule 2); a `workflow` block additionally opens the workflow context for subsequent `step` blocks in the same file. A step's qname is `<workflow qname>.<step name>`; when the workflow block is the file's scoping block its qname is its bare name.
Rationale: §7.3/§7.4 were not implementable as written against the actual tree-sitter grammars; §7.5's scoping blocks structurally cannot satisfy §7.3's binding requirement.
Affects: §7.3, §7.4, §7.5.

**D-043 -- Manifest value validation.**
Decision: New code `E0403`: a known manifest key with an invalid value (unknown language name, non-string glob target, bad policy value, unparseable TOML). `E0401` stays reserved for unknown *keys*. Added to §18 band E040x.
Rationale: §11 validated key names but said nothing about value domains.
Affects: §11, §18.

**D-044 -- Binder node search tolerates same-row container nodes.**
Decision: To find the subject, the binder walks the parse tree along the single descent path containing the subject line and considers, shallowest first, every named node that starts exactly on that line; the subject is the first such node that (after wrapper descent per D-042) is a §7.4 declaration node. When none qualifies, `E0102` names the shallowest node's kind.
Rationale: In tree-sitter-python a class-body `block` begins on the same row as its first statement, so a pure "shallowest node starting at the line" rule resolves to the container and cannot bind a method at the top of a class body. The container is not a §7.4 wrapper (it does not wrap a single declaration), so the skip set is the wrong mechanism; the search itself must look past same-row containers.
Affects: §7.3.

## T2 implementation clarifications

**D-045 -- Clause-parser diagnostics and semantics.**
Decision: (a) New code `E0207` malformed clause: a clause line whose value does not match its §15 production (missing or unterminated string, bad or empty ref list, invalid route method, invalid enforcement value, missing `:`, trailing text after the value). `E0202` stays reserved for a well-formed `<name>:` line whose name is not in the §3.1 clause set; its message always suggests the nearest valid clause by edit distance. (b) String escapes: `\"` and `\\` are unescaped; any other `\` sequence is kept verbatim. (c) A clause that fails to parse contributes nothing to `Intent` -- no partial ref lists; a half-parsed claim is a guess (G-7). A duplicate singular clause (`E0206`) keeps the first occurrence and drops the repeat. (d) Value spans: a prose value's span covers its string literal including quotes; each ref in a ref list carries its own span; an enforcement value spans its keyword; a route value spans from the method (or opening quote) to the closing quote. Whitespace-only clause lines are skipped silently (a bare `#` inside a block is a visual separator, not a clause).
Rationale: §7.2/§15 define the clause grammar but name diagnostics only for unknown (`E0202`) and duplicate (`E0206`) clauses; the unhappy path (G-11) needs a code for every way a clause value can be malformed, and downstream lint (`E0306`/`W0301`) needs per-ref and per-value spans to point at.
Affects: §7.2, §18.

## T3 implementation clarifications

**D-046 -- Graph inputs, ambient manifest modules, and requirement-check scope.**
Decision: `lore_graph::build` consumes (a) the declared `IntentNode`s produced from annotation blocks (clauses already parsed) and (b) the manifest's `[modules]` names. Each distinct manifest module name not already declared by a scoping block becomes a Module node (origin `Declared`, empty intent, loc = the manifest file), per §4's "lore.toml mapping" origin row. Requirement-class checks -- `E0201` missing required intent, `W0209` missing recommended purpose, `E0204` empty step -- apply only to nodes declared by an intent block; ambient manifest modules (and, from T6, derived-only nodes) are exempt.
Rationale: §4 makes a lore.toml mapping originate a Module node, but §3.2's required-intent rows would then make every mapping-only module an instant `E0201`, destroying cold start (D-002). A node with no block has nowhere to carry clauses; the requirement attaches to the act of declaring.
Affects: §3.2, §6.3, T3 lint.

**D-047 -- Structural edges and resolution mechanics.**
Decision: (a) `Contains`: a node's immediate container is the longest proper qname prefix present in the node table whose kind is Module, Service, or Workflow; one edge container→member. `Sequence`: consecutive Step nodes of the same workflow, in source order. Both structural edge kinds carry layer `Derived` with confidence `Exact` -- §6.2 demands exactly one of status/confidence, and structure is fact, not claim. (b) A ref that fails resolution (`E0306`) or the kind check (`E0307`) produces no edge -- a wrong edge is poison (G-7). Refs in a clause that is illegal for the node's kind (`E0203`) are not resolved and produce no edges. (c) §6.3's `Graph.unresolved` is realized as the `E0306` findings in `Graph.findings`; no separate field is added to the §13 contract, and the §10.4 `unresolved` array is derived from those findings. (d) Edge `loc`: a claim edge carries its ref's span; a `Contains` edge the member's loc; a `Sequence` edge the successor step's loc. (e) Until `lore_derive` exists (T6) the derivation scope is empty, so §9.1 labels every claim `Unverifiable`; this is the §9.1 algorithm applied to an empty scope, not a deviation.
Rationale: §6.1 names Contains/Sequence "derived from structure" but no container rule or layer labeling existed; §6.3 referenced a `Graph.unresolved` field absent from §13.
Affects: §6.1, §6.2, §6.3, §13.

**D-048 -- `depends_on` surface and module-locality definitions.**
Decision: (a) The **owner chain** of a target t is every prefix of t's qname (t itself included) that names a Module, Service, or External node. (b) A construct C's **effective depends_on** is the union of C's own `depends_on` refs and those of every node whose qname is a proper prefix of C's (its containers). (c) A clause ref (`affects`/`reads`/`triggers`/`emits`/`on`) from C to t is **module-local** iff some element of t's owner chain is a prefix of C's qname; module-local refs need no declared dependency. A non-local ref is `E0304` unless t's owner chain intersects C's effective depends_on. Targets with an empty owner chain (orphans) never fire `E0304`. (d) A `depends_on` entry d on C is **used** (no `W0206`) iff some clause ref from C or from a node contained in C resolves to a target whose owner chain contains d; an entry that did not resolve (`E0306`) is not additionally reported as unused. (e) `W0205`'s "same module" means: source and target have the same nearest enclosing Module node.
Rationale: D-008 defines both directions of the surface check but left "effective", "owning module", and module-locality undefined; the checks are unimplementable without them.
Affects: §5, §6.3, T3 lint.

**D-049 -- `enforcement: strict` promotion mechanics.**
Decision: Every graph finding is attributed to the node it is reported on. Promotion looks up that node's nearest enclosing Module (the node itself when it is a Module; otherwise the longest proper qname prefix of kind Module -- never higher, per §5's no-inheritance rule); if that module declares `enforcement: strict`, findings with a `W` code in bands 02x/03x are promoted to severity Error with the code string unchanged. Scanner-band (01x) and manifest-band (04x) findings are never promoted.
Rationale: §18 names the promotion but not the attribution or the nearest-module rule.
Affects: §18, T3 lint.

**D-050 -- Rust scanner+binder activation (G-12 dogfooding).**
Decision: (a) Extensions: `.rs`; comment token `//`. (b) The subject identifier is the `name` tree-sitter field for all seven §7.4 Rust declaration nodes (`function_item`, `struct_item`, `enum_item`, `trait_item`, `static_item`, `const_item`, `mod_item`). (c) `attribute_item` is a **preceding-sibling skip**, not a descend wrapper: in tree-sitter-rust, attributes precede the declaration as sibling nodes, so when the node after a block is an `attribute_item` the binder advances along named siblings past consecutive `attribute_item`s and binds the first declaration node it reaches; the subject span starts at the declaration node, attributes excluded. (d) Outer doc comments (`///`) between a block and its subject are not skipped; the dogfooding pattern is doc comments first, a blank line, then the `@lore` block directly above the declaration (Rust doc comments attach to the following item across comment trivia, so this is lossless).
Rationale: D-042 listed identifier fields for Python/TS only, and §7.4's wrapper/skip framing assumes the wrapper *contains* the declaration, which is false for Rust attributes.
Affects: §7.3, §7.4 (extends D-042).

## T4 implementation clarifications

**D-051 -- Query syntax details: filter arguments, KindName, string literals.**
Decision: (a) The arguments of `in module(...)`, `in service(...)`, and the `unknown in <scope>(...)` form are `ref` (dotted), not bare `Ident` -- §10.1 updated. A scoping block may declare a dotted name (`name: A.B`, §7.2), so a bare-Ident filter could never name such a module. (b) `KindName` is one of the ten lowercase §7.2 kind keywords (`module` ... `external`). (c) String literals in queries follow the §15 `StringLit` production with D-045b escape semantics (`\"` and `\\` unescaped, other `\` sequences kept verbatim).
Rationale: §10.1 left filter argument shape, `KindName`, and query-string escaping undefined; the parser is unimplementable without them.
Affects: §10.1.

**D-052 -- Query semantics over the node table.**
Decision: (a) Scope membership: a node N is in scope X (filter `in module(X)`/`in service(X)`, or `unknown in <kindword>(X)`) iff X's qname is a prefix of N's qname (X itself included) and X names a node of the scope's kind. (b) **Effective owner** (for `owner("t")` and `owned_by("t")`): a node's declared `owner`; for State and Event nodes (whose owner is inherited, §3.2) the declared owner of the nearest enclosing Module. No other kind inherits. (c) `tagged("t")` returns the empty set in Phase 1: tags are a Phase 2 type attribute (§14.1) and no Phase 1 surface declares them -- an honest empty answer, not an error. (d) `unknown` returns the nodes carrying at least one `unknown` clause (intersected with the scope and filters); human output prints each unknown string on an indented line under its node; in `--json`, results of this query form additionally carry an `"unknown"` array of the node's unknown strings. (e) Node-set results never include the query argument itself, are deduplicated by qname keeping the shortest witnessing chain, and are sorted by qname. (f) `show(X)` and `path(A, B)` take no trailing filters (usage error); every other query form intersects filters per D-023.
Rationale: §6.4's "node attribute scan / node scans" rows and D-023's filters were named but not defined over the actual node table; the `unknown` query exists to surface the texts, which the §10.4 result shape had no field for.
Affects: §6.4, §10.1, §10.3, §10.4.

**D-053 -- `lore ask` failure modes, exit codes, and the `unresolved` array.**
Decision: (a) Exit 2 (usage/input error, §10.5) with a message on stderr for: a query that does not parse; a query or filter argument that names no node (the message names the nearest existing qname by edit distance, mirroring E0306); a filter argument naming a node of the wrong kind; `--all` without `--max-len`; filters on `show`/`path`. (b) Otherwise `lore ask` exits 0 -- graph findings never fail `ask` (lint owns the exit-1 surface; `ask` answers questions). (c) The §10.4 `unresolved` array is present on every response and carries the dotted refs of the graph's `E0306` findings (D-047c), deduplicated and sorted -- the honest "my answer may be incomplete" signal. (d) The §10.6 `traverse` primitive's `transitive` parameter is realized as a three-valued mode (single-hop, transitive, all-simple-paths-bounded); `path --all` is the bounded mode, still the same primitive, not a new engine code path.
Rationale: §10/§12 specify `ask`'s success shape but no failure behavior; D-047c said the `unresolved` array derives from E0306 findings without fixing item shape or presence.
Affects: §10.4, §10.5, §12.

**D-054 -- Witness chains: `via` ordering, event-hop rendering, `path --all`.**
Decision: (a) A result's `via` is the ordered edge list of its shortest witnessing chain, every edge in its stored `from`/`to` orientation, ordered causally (upstream first) -- so the chain runs result→argument for reverse queries and argument→result for forward ones. (b) The composite event hop contributes both constituent edges, `Emits` then `Handles` (consecutive entries share the Event node; the Event node itself does not enter the result set -- §6.4 traverses the hop, not bare `Emits`/`Handles`). (c) Human output appends the per-hop trust labels in chain order: `[via: Triggers -> Affects]  [Declared/Unverifiable -> Declared/Verified]` (§6.4's "label which hops were declared vs derived" applied to §10.3's line shape). (d) `path(A, B)` returns one result per witnessed path, each with qname B and that path's `via`; default is the single shortest path; `--all --max-len N` enumerates all simple paths of at most N edges, where the event hop counts as its two edges.
Rationale: §10.3/§10.4 show single-edge `via` examples only; multi-hop ordering, composite-hop rendering, and the multi-path result shape were undefined.
Affects: §6.4, §10.3, §10.4.

**D-055 -- Finding attribution becomes part of the Graph contract.**
Decision: `Graph` gains `pub attributions: HashMap<QName, Vec<usize>>` -- for each node, the indices into `Graph.findings` of the findings attributed to it (the same attribution D-049 defined for strict promotion, now public). §13 updated. `show(X)`'s "findings attached to the node" (§10.2) renders exactly these.
Rationale: §10.2 requires per-node findings but §13's `findings: Vec<Finding>` carries no attribution, and span-matching cannot recover it (a clause finding's span lies in the block, outside the subject span).
Affects: §10.2, §13.

## T5 implementation clarifications

**D-056 -- `[lint]` severity overrides: key/value domains and application point.**
Decision: (a) A `[lint]` key MUST be a `W`-prefixed code from the §18 registry. Any other key is `E0401` with a message stating that only W-codes can be overridden -- E findings can never be silenced (G-7), and a typo'd code must fail loudly rather than silently fail to suppress. (b) Values are `"warn"` or `"off"`; anything else is `E0403`. `"warn"` restates the default and is a no-op -- it does not demote findings promoted by `enforcement: strict` (D-049) or by `[policy]`. `"off"` suppresses every finding with that code from lint output and from the exit-code computation, **including** instances promoted to Error: the code names the check, and turning the check off disables its reporting everywhere. (c) Overrides apply at the lint reporting surface (`lore lint`), over the merged scanner+parser+graph findings, after policy promotion. `lore ask`/`show` render the graph's findings unfiltered -- overrides shape the CI surface, not the graph.
Rationale: §18 names the mechanism ("override W↔off") but left the key/value domains, their diagnostics, and the interplay with promotion undefined; the manifest layer validates value domains per D-043.
Affects: §11, §18, T5 lint.

**D-057 -- Surfacing `unknown` clauses: new code `W0213`, `[policy] unknown` promotion.**
Decision: New code `W0213` in band 02x (0210--0212 are taken by the D-026 hygiene checks): one finding per `unknown` clause occurrence, emitted by `lore_graph::build`, span = the clause's value span, message carrying the unknown text verbatim, attributed to the declaring node -- so `enforcement: strict` promotes it (D-049) and `show(X)` renders it. `[policy] unknown = "error"` promotes every W0213 to severity Error with the code unchanged (mirroring D-049's promotion shape), applied at the lint surface where the manifest lives; the graph always carries the base Warning.
Rationale: §5 says unknown severity is "per `[policy] unknown` (D-012)" and the T5 scope is "[policy] promotion for unknown", but no code existed for an unknown clause -- promotion needs something to promote. Default `warn` keeps a declared unknown permanently visible in CI without failing it: surfacing honesty, not punishing it (G-7).
Affects: §5, §18, T5 lint.

**D-058 -- `W0207` CODEOWNERS cross-check mechanics.**
Decision: (a) Discovery, in `lore_cli` next to the manifest: the first existing of `.github/CODEOWNERS`, `CODEOWNERS`, `docs/CODEOWNERS` (GitHub's own search order). (b) The file parses into ordered rules (pattern + owner tokens; `#` comments and blank lines skipped; lines with no owner tokens are kept as explicitly-unowned rules); the **last** matching rule wins, per CODEOWNERS semantics. (c) v1 pattern subset, translated to path globs: a leading `/` anchors at the project root, otherwise the pattern matches at any depth; a trailing `/` matches everything under that directory; a pattern without a wildcard also matches as a directory prefix. (d) The check runs in `lore_graph::build`, which takes the parsed `Codeowners` (file path + rules) as **data** -- `lore_cli` reads and parses the file; the graph crate never touches the filesystem. Checked nodes: those with a declared `owner` clause whose `loc.file` is matched by some rule. (e) An owner token matches the declared owner when, after stripping a leading `@`, the whole token or its last `/`-segment equals the declared string ASCII-case-insensitively (so `@org/payments-team` matches `owner: "payments-team"`). A winning rule with owner tokens none of which match is `W0207`, naming the CODEOWNERS file, its owners, and the declared owner; a winning rule with no owner tokens never fires (nothing is contradicted).
Rationale: D-010 mandates the check but file discovery, pattern semantics, owner-string comparison, and the executing crate were undefined; full gitignore semantics are out of v1 scope, and a skipped pattern only skips a *warning* -- never a wrong edge (G-7).
Affects: §5 (owner), §13 (build input), T5 lint.

**D-059 -- `lore history` mechanics: resolution, git invocation, output, failure modes.**
Decision: (a) `lore history <qname> [--json]`. The qname must name a graph node; otherwise exit 2 with the D-053a message shape (nearest existing qname by edit distance). (b) The span rendered is the node's `loc` (`line`--`end_line` of `loc.file`, manifest-relative). Invocation: `git -C <manifest dir> log -s --date=iso-strict --format=... -L<start>,<end>:<file>`; `-s` suppresses the patch; hash, author, author date, and the full message are read via field separators. (c) Outside a git work tree, or on any git failure (untracked file, span past EOF, git absent), exit 2 with the underlying message on stderr -- unlike staleness (§9.2, which skips with a notice), history without git has no answer at all. An empty log is an honest empty answer: exit 0. (d) Human shape: header `history for <qname>  <file>:<start>-<end>: N commits` (dropped under `--quiet`), then per commit one line `<hash[..12]>  <author date, ISO-strict>  <author>` followed by the full message indented four spaces. JSON shape: `{lore_version, qname, location {file, line}, span {start, end}, commits: [{hash (full), author, date, message}]}`.
Rationale: §9.3 specifies the data source but not argument failure modes, the exact git invocation, machine output, or behavior outside git; D-037's `lore_history` MCP tool requires a JSON shape to exist.
Affects: §9.3, §12.

## T6 implementation clarifications

**D-060 -- Derived node set, qnames, and collision handling.**
Decision: (a) §8.1's "every declaration-table node" maps as: function/method declaration forms → `Function`; class, interface, type-alias, and enum forms → `Type`. Value-binding forms (Python `assignment`, TS `lexical_declaration`/`variable_declaration`) derive **no** node -- §8.1 assigns them no kind; they enter the graph only through an annotation (`kind: state` and friends). Nesting does not restrict the set: methods and nested functions derive too. (b) Derived qname = file module (§7.5) + host identifier, flat regardless of nesting -- exactly the binder's rule, so an annotation and the declaration it binds to collide by construction and merge. Merging requires the *same declaration*: same file and same declaration-node start line; the merged node keeps the declared kind, intent, and loc, with origin `Both`. (c) A derived node whose qname collides with a declared node of a *different* declaration is `E0305` (the declared node wins; the derived node and every derived edge touching its qname are dropped). (d) Two or more *derived-only* declarations sharing a qname (`__init__` in two classes of one module, same-named helpers in two functions) produce **no finding**: every colliding declaration and its edges are excluded from the graph, counted in `lore stats` as `ambiguous_derived_names`.
Rationale: an error finding for (d) would make `lore lint` fail on virtually every unannotated real repo (two Python classes suffice), destroying cold start (D-002); merging or picking one would poison every edge through the node (G-7). An honest gap, counted in stats, is the only option satisfying both. This also refines D-002's "StateTouch edges with zero annotations": §8.3 targets are *declared* State nodes, so cold start delivers Function/Type nodes and Calls edges; `affects(X)` additionally needs X's own state annotation (roadmap T6 criterion reworded).
Affects: §8.1, roadmap T6, `lore stats`.

**D-061 -- Derivation scope and per-file module assignment.**
Decision: the derivation scope is the set of files whose language has derived-layer support (Python, TypeScript at T6; D-014) AND that are assigned a module by either §7.5 mechanism -- a `[modules]` glob or a top-of-file scoping block, whose name override applies to derived qnames too. Orphan files are outside scope. §2's glossary entry is updated accordingly. `lore_annotations::scan` exposes the per-file module assignment it already computes, so the CLI builds the scope without re-implementing §7.5.
Rationale: the glossary said "mapped to modules by lore.toml", but a scoping-block file's members are first-class module members under the same qnames; excluding such files would label claims about them `Unverifiable` for no reason and split the qname space between the two layers.
Affects: §2, lore_annotations boundary, lore_cli.

**D-062 -- Derived edge mechanics: attribution, drops, imports, dedupe.**
Decision: (a) Calls and state touches attribute to the **nearest enclosing derived Function node**. A call with no such node -- module or class level, inside a value-bound lambda/arrow, or inside a declaration dropped by D-060d -- is dropped and counted in `unresolved_calls` (one drop counter; an attributed guess is poison, G-7). (b) A call that resolves to a non-`Function` node (class constructors) is dropped and counted -- §6.1 fixes `Calls` as function→function. (c) Import resolution, v1: Python `import m [as a]` and `from m import n [as a]` (one alias level, per §8.2's "aliases beyond one level" exclusion; relative imports drop); TypeScript relative `./`/`../` specifiers resolving to `<p>.ts`, `<p>.tsx`, `<p>.js`, or `<p>/index.ts`, with named imports `{ n [as a] }` and namespace imports `* as m` (default imports drop). A dotted callee deeper than `alias.name(...)` drops. (d) State-touch occurrences match the state's host identifier: bare in same-language files of the owning module, bare via a named import of it, or as `alias.identifier` via a whole-module/namespace import -- in the import forms only when the import resolves to the state's defining file. Same-language is required for the bare own-module form: a coincidental identifier in another language's file is exactly the wrong-edge risk G-7 exists for. Occurrences with no enclosing derived function (module level, including the state's own definition) produce no edge and no counter -- there is nothing to attribute. Touch edges dedupe to one edge per (function, state, kind); the first occurrence's span wins. (e) Method-call resolution (§8.2 last rule): `x.m(...)` resolves `Exact` when `x` is bound to a direct construction of a same-file class in the same function (`x = Foo()` / `const x = new Foo(...)`) and `m` is that class's method; every other method call drops.
Affects: §8.2, §8.3.

**D-063 -- Claim statuses between T6 and T7 (supersedes D-047e).**
Decision: with derivation live but reconciliation a T7 deliverable, `lore_graph::build` applies §9.1 minus its `Contradicted` branch: target outside derivation scope → `Unverifiable`; matching derived edge (same-kind for `Affects`/`Reads`, derived `Calls` for `Triggers`) → `Verified`; otherwise `Unverified`. `Emits`/`Handles`/`DependsOn` claims stay `Unverifiable` (Phase 1, §9.1). Until T7, a claim that the symbol-occurrence test would prove `Contradicted` surfaces as `Unverified`, and no `W0302`/`E0302` is emitted.
Rationale: labeling an in-scope verified claim `Unverifiable` would be a false statement about scope once derivation exists, and the `Verified` branch is certain and free; the `Contradicted` branch needs the T7 source-text test, and `Unverified` is a withheld verdict, never a false alarm (G-7).
Affects: §9.1 (transitional note), lore_graph.

**D-064 -- Parse cache realization: facts, not trees (G-9, §10.7).**
Decision: the `.lore-cache/` cache stores per-file **extraction facts**, not serialized parse trees: tree-sitter trees are not serializable, and the facts (declarations, raw call sites, imports, classified state-symbol occurrences) are exactly what downstream consumes. Layout: `.lore-cache/derive/<hash>.json` next to lore.toml; the key is a 64-bit FNV-1a over (cache format version, language, relative path, file content, module name, import roots, applicable state-symbol descriptors). All cross-file work -- import resolution, callee lookup, D-060d ambiguity drops -- is recomputed from facts on every run because it depends on *other* files; caching it would serve stale edges. Unreadable, corrupt, or key-mismatched entries are re-derived silently; the directory stays safe to delete (§10.7).
Affects: §8.5, §10.7.

**D-065 -- `lore stats` lands at T6 with the derivation counters.**
Decision: T6 ships `lore stats [--json]` reporting: node counts by kind and origin, declared-intent coverage per kind (share of nodes carrying at least one intent clause), edge counts by layer, `unresolved_calls` (§8.2 rule 3), and `ambiguous_derived_names` (D-060d). The claims-by-status breakdown is T7 scope and joins the command then. §12's milestone cell for stats becomes "T6 (counts) → T7 (claim statuses)". JSON field names are pinned by the CLI tests.
Rationale: the T6 exit criteria require `lore stats` to report `unresolved_calls` and per-kind node counts while §12 listed the command at T7; the roadmap's criteria are binding (G-6), so the command lands now with its T6-knowable subset.
Affects: §12, T6/T7 scope split.

## T7 implementation clarifications

**D-066 -- Reconciliation mechanics: build inputs, the symbol-occurrence test, and the W0302/E0302 code choice (supersedes D-063).**
Decision: (a) The full §9.1 algorithm, `Contradicted` branch included, runs in `lore_graph::build` from T7; the D-063 transitional behavior ends and §9.1's transitional note is removed. (b) `build` gains a `ReconcileInput` (§13): `sources` (file → text for every scanned file) and `host_identifiers` (declared node qname → the binder's extracted subject identifier) -- the CLI supplies both as data; the graph never reads the filesystem (D-058 precedent). (c) **t's bound host identifier** is: for nodes declared by an annotation, the `host_identifiers` entry (the identifier as written in source -- a `name:` override changes the qname, never the matched symbol); for derived-only nodes, the last qname segment (which D-060b makes the host identifier by construction). A target with neither (scoping blocks, ambient manifest modules, subjects whose identifier was not extractable) has no bound symbol, so the occurrence test cannot run and the claim can never be `Contradicted` -- it stays `Unverified` (a withheld verdict, G-7). The same holds when f's file is missing from `sources`. (d) **Token match:** an occurrence is a maximal run of `[A-Za-z0-9_]` in the raw text of f's subject span (`loc.line..=loc.end_line` of `loc.file`) equal to the identifier. Raw text means comments and strings count: deliberately conservative -- any mention withholds the verdict rather than risking a false alarm. (e) A `Contradicted` claim emits one finding at the ref's span, attributed to f: code `E0302` (severity Error) when f's nearest enclosing Module (D-049 rule) declares `enforcement: strict`, else `W0302` (Warning) -- per D-019 the *code itself* switches, unlike D-049 promotion which keeps codes; consequently D-049's blanket promotion never sees a W0302 in a strict module. `[lint] "W0302" = "off"` therefore suppresses only warn-level instances: the strict instances are E findings, and E findings can never be silenced (D-056a). (f) The §9.1 first branch tests only the *target's* file against the derivation scope; f's own span text is readable regardless of language support, so a claim from an unsupported-language file can still be `Contradicted` -- the occurrence test is text, not analysis.
Rationale: §9.1 names the test but not its inputs, the identifier for each target class, the token definition, or which crate reads source text; D-019's two codes for one check needed reconciling with the D-049/D-056 promotion machinery.
Affects: §9.1, §13, lore_graph, lore_cli.

**D-067 -- `W0303` undeclared-effect mechanics.**
Decision: (a) One `W0303` per derived `Affects` edge f→t where f is a node declared by an intent block (the D-046 `annotated` set) and the graph holds no declared `Affects` edge f→t of any status. Span = the derived edge's loc (the write site); attributed to f; the message prints the edge's confidence (G-7). Derived `Reads` edges never fire it -- §9.1 names writes only, and the read heuristic is the noisiest classification. D-062d's dedupe (one touch edge per function/state/kind) bounds it to one finding per claim gap. (b) Mirroring D-057, the graph always emits the base Warning; `[policy] undeclared_effects` applies at the lint surface: `"off"` (the default) drops every W0303 from lint output and the exit computation, `"warn"` keeps them. `ask`/`show` render the graph's findings unfiltered (D-056c). D-049 strict promotion applies when the policy exposes them.
Rationale: D-019 fixes the default and the punish-low-coverage rationale but not the executing crate, the "corresponding declaration" test, or where the policy applies; the D-057 graph-carries-base/surface-applies-policy split keeps the graph manifest-free.
Affects: §9.1, T7 lint.

**D-068 -- Staleness mechanics: who runs git, what is compared, where W0301 lands.**
Decision: (a) Split per the D-058/D-059 precedent: `lore_cli` gathers git metadata (it owns process boundaries); `lore_graph::build` applies the §9.2 comparison and emits `W0301`, so attribution, D-049 strict promotion, and `show(X)` rendering work unchanged. `ReconcileInput.staleness: Option<Vec<StalenessRecord>>` carries the metadata; `None` means the check is skipped. (b) A `StalenessRecord` carries the block's qname, the block span (the finding's location), `t_block` and `t_subject` as unix seconds, their ISO-strict renderings, and the subject's most-recent commit hash. The graph emits W0301 iff `t_subject > t_block` (strictly later), reporting both timestamps and the hash. (c) Gathering: only `lore lint` gathers (and not under `--no-stale`); `ask`/`stats`/`history` pass `None` -- blaming the repo per query would break the §10.7 latency budget, and §9.2 is a CI check. Work-tree detection via `git rev-parse --is-inside-work-tree`; outside one (or with git unrunnable) staleness is `None` and lint prints one stderr notice, suppressed by `--quiet`. (d) Per file containing blocks with a subject span, one `git blame --line-porcelain` over the work-tree state; line time = **committer-time** (commit time per D-018; author time survives rebases that staleness must see). `t_block` / `t_subject` are the max over the block / subject-span lines; the subject hash is the first subject line attaining the max. Uncommitted lines blame to the zero hash at the current time under one clock, so editing block and subject together ties -- and ties are not stale. A file whose blame fails (untracked) is skipped silently: with no history there is nothing to be stale against. Scoping blocks have no subject span and are never checked. (e) `[policy] stale = "error"` promotes W0301 to severity Error at the lint surface, code unchanged, mirroring D-057.
Rationale: §9.2 fixes the formula but not the executing crate, the blame field, tie/uncommitted/untracked behavior, or which commands pay the git cost.
Affects: §9.2, §13, T7 lint.

**D-069 -- `lore stats` claims-by-status breakdown shape.**
Decision: `lore stats` gains a claims section counting every declared edge that carries a `ClaimStatus` (all six claim kinds), reported as total plus the four statuses in §6.2 order. Human output: one `claims by status:` line after the kind table; JSON: `"claims": {"total": N, "verified": N, "unverified": N, "contradicted": N, "unverifiable": N}` (field names pinned by CLI tests, like the D-065 counters).
Rationale: D-065 deferred the breakdown to T7 without fixing its shape; the §12 row requires it now.
Affects: §12, lore stats.

## T8 design decisions

**D-070 -- Language packs: per-language adapters become data.**
Decision: (a) From T8 a supported language is defined by a **language pack**: a directory `packs/<name>/` at the workspace root containing `lore-lang.toml` (the pack manifest), `queries/bind.scm`, `queries/derive.scm` (tier-dependent), and a mandatory `fixtures/` conformance suite. One generic adapter consumes packs; per-language Rust modules are banned (sole exception: named custom import strategies, D-071). §8.5's `lore_derive/src/lang/<name>/` layout is superseded, and its five artifacts map onto pack files: grammar dependency → `[grammar]`; declaration queries → `bind.scm`; call-expression queries → `derive.scm`; import-resolution rules → `[derive.imports]` strategies (D-071); mutator list → `[derive] mutators`. Format normative in new spec §8.6. (b) **Tiers**, cumulative, declared as `tier = "scan" | "bind" | "derive"`: at `scan` only the scanner runs (comment token + extensions; no grammar) -- scoping blocks work in full, and a non-scoping block MUST carry `name:` (new `E0109`) and binds to no subject: qname = §7.5 module + name, loc = the block span, no subject span -- so it is never staleness-checked (D-068d already exempts spanless blocks) and its claims can never be `Contradicted` (D-066c: no extractable identifier); the file never enters derivation scope. At `bind` the grammar + `bind.scm` activate: full §7 binding, subject spans, `E0102`. At `derive` the file enters derivation scope (D-061's "derived-layer support" now means tier `derive`). The declared tier MUST match the artifacts present in both directions: a missing artifact at the tier, or an artifact above it, is `E0410` -- the manifest is an honest statement of capability. (c) **Versioning and grammar forward-compatibility:** `[pack] format` is a required integer, starting at 1; an unknown version is `E0412` and the pack is refused before anything else is read. `[grammar]` is a table: v1 accepts only `source = "builtin"` + `name` (statically-linked grammar id); `source = "wasm"` + `path` is **reserved key space**, recognized and refused with `E0413` -- accepting WASM later widens a value domain, never changes the format. (d) **Loading split**, per the D-058/D-066 precedent: `lore_cli` embeds the builtin packs at build time, parses and structurally validates them (band `E041x`), and passes each pack to `lore_annotations` and `lore_derive` as pure data (`PackSpec`) plus the tree-sitter `Language` handle as a separate argument -- `lore_intent` gains no tree-sitter dependency. The generic adapters compile `bind.scm`/`derive.scm` at activation; a compile failure or unknown capture name is `E0411`. Grammar crates become dependencies of `lore_cli` only (dev-dependencies elsewhere for boundary tests), still pinned in the workspace (roadmap risk 4 unchanged). `PackSpec` joins §13 in the T8 implementation PR (G-3: the contract lands in the same commit as its first consumer); its semantic content is fixed by §8.6 now. (e) **Conformance gate -- G-7 made structural:** a pack MUST ship the §8.6.4 mandatory fixture classes for its tier, negative fixtures included (comment text that must not scan, annotations that must not bind, calls that must be dropped, occurrences that must not classify as writes). A pack failing structural validation, or whose suite has not passed for its exact content, MUST NOT be activated -- the language is simply not loaded, never partially. For builtin packs the enforcement point is the conformance harness, a `lore_cli` test that runs every embedded pack's suite through the real scan→bind→derive pipeline in CI, so a failing pack cannot ship; for future external packs the suite runs at first load (verdict cacheable by content hash -- reserved, like WASM). A missing or empty mandatory class, or a fixture mismatch where the suite runs at load, is `E0415`. (f) **Diagnostics:** new §18 band `E041x` -- `E0410` invalid pack manifest (unknown/missing key, invalid value, tier/artifact mismatch, one extension claimed by two packs), `E0411` unusable pack artifact (missing or unreadable file, query compile failure, unknown capture name), `E0412` unsupported pack format version, `E0413` grammar unavailable (`wasm` in v1, unknown builtin name), `E0414` unknown or misconfigured import strategy (D-071), `E0415` conformance failure; plus `E0109` in band 01x (scan-tier non-scoping block missing `name:`). (g) **§7.4 stays normative** (D-013 upheld): a builtin pack MUST realize exactly its §7.4 row, enforced by its conformance fixtures. The per-language *data* of D-042(b)/D-050(a,b) relocates into pack files with values unchanged; the binder *mechanics* (D-042/D-044 wrapper descent and same-row search, D-050 sibling skips) remain engine behavior, parameterized by the pack's `wrappers`/`sibling_skips` lists. (h) **Python and TypeScript migrate** onto packs at T8: their existing fixture suites become their conformance suites and every pre-T8 test passes unchanged -- one adapter, zero behavioral drift. (i) The D-064 cache key gains the pack identity (pack name, `format`, content hash over `lore-lang.toml` and both `.scm` files): extraction facts are now a function of pack content (extends D-064's key tuple).
Rationale: most of an adapter is tables, query patterns, and word lists -- data with a checkable format -- while the hard parts (binder search, resolution, drop rules, confidence) are language-independent engine logic that should exist exactly once. Five hand-written adapters drifting apart is precisely the wrong-edge risk G-7 names; a mandatory conformance protocol with negative fixtures turns roadmap risk 1's "ship it scanner+binder-only and say so" from a judgment call into a tier declaration, and turns adding a language into work a contributor can do without writing Rust.
Affects: §7.4, §8.2, §8.5, new §8.6, §13 (T8 PR), §18, roadmap T8; extends D-042/D-050 (data relocated, values unchanged) and D-064 (cache key).

**D-071 -- Import resolution: built-in strategy library, pack-selected, with a named-impl escape hatch.**
Decision: (a) Import/reference resolution (§8.2 rule 2) becomes a built-in library of four strategies, selected and parameterized per pack as `[[derive.imports.strategy]]` -- an ordered array, tried in order, first resolution wins; when no strategy resolves, the reference drops and is counted (§8.2 rule 3, unchanged). The strategies: **`relative`** -- specifiers beginning `./`/`../` against the importing file's directory; params `extensions`, `index_files`. **`root_relative`** -- dotted or path-shaped module names against `lore.toml [project] roots`; params `separator`, `extensions`, `init_files`. **`package_dir`** -- same-directory sibling files, serving bare same-package references and same-package imports; param `extensions`. **`manifest_prefix`** -- strip the module prefix declared in a language manifest found by walking up from the importing file (params `manifest_file`, e.g. `"go.mod"`, and `prefix_key`, e.g. `"module"`), then resolve the remainder as a directory path under the manifest's directory. (b) Escape hatch: `kind = "custom"`, `name = "<id>"` selects a Rust `ImportStrategy` trait impl from a registry in `lore_derive`. Every custom strategy requires its own D-entry stating why no builtin (or parameterization of one) fits -- the mirror of the query-form sugar rule (roadmap risk 5). (c) v1 language mapping, preserving §8.2/D-062c semantics exactly: Python = `root_relative` (relative imports stay dropped: the pack simply does not configure `relative`); TypeScript = `relative` (default imports stay dropped: `derive.scm` does not capture them, and uncaptured forms drop -- G-7); Go = `package_dir` + `manifest_prefix("go.mod")`; Java = `package_dir` + `root_relative` (explicit single-type imports are root-relative paths); Rust = custom **`rust_use_paths`**, the first registered custom strategy: `use crate::`/`self::`/`super::` paths resolve through the `mod`-declaration tree, which is not a function of the directory tree, so no path-shaped strategy can express it -- exactly what the hatch is for. (d) Strategies are pure functions over (reference specifier, importing file path, project data: roots, candidate file set, manifest texts); they never read the filesystem -- `lore_derive` supplies the data (D-058 precedent).
Rationale: D-062c already showed that import rules are per-language *data* arranged around one shape (specifier → candidate files); naming the four shapes makes the next language a TOML stanza, while the named-impl hatch keeps genuinely irregular module systems out of the data format instead of bloating it. A strategy that exists as Rust under a D-entry is reviewable; the same logic encoded as stringly TOML patterns would not be.
Affects: §8.2, §8.6, lore_derive (T8), roadmap T8.

**D-072 -- `derive.scm` call vocabulary: member-call decomposition and the local-construction binding.**
Decision: §8.6.3's `derive.scm` call vocabulary, as first written, listed only `@call` / `@call.callee` and so could not express the two call shapes §8.2 resolution depends on: a member call `recv.method(...)` (whose receiver and method §8.2 rule 1 / D-062c resolve separately) and the local construction `x = Cls()` / `const x = new Cls(...)` that the D-062e method-call rule binds. The pre-T8 per-language code reached into the callee node with `child_by_field_name` and matched a per-language `@construct`/`@var`/`@cls` triple; under packs (D-070) both must be captures, not adapter special cases. The vocabulary gains, all within the fixed set (an unknown capture is still `E0411`, D-070d):
(a) **Member calls.** A `@call` whose callee is a member/attribute access captures `@call.receiver` (the object identifier) and `@call.method` (the member identifier) in place of `@call.callee`. So: `@call` + `@call.callee` is a bare call (callee identifier, §8.2 rule 1 bare/imported forms); `@call` + `@call.receiver` + `@call.method` is a member call; `@call` with neither resolved capture is opaque and drops (§8.2 rule 3). Receivers deeper than one identifier (`this.x.m()`, chains, computed members) are simply not captured -- they fall to the opaque/drop path, exactly the D-062c "dotted deeper than alias.name drops" rule, now expressed by the pattern, not by Rust (G-7).
(b) **Local construction.** `@call.construct` marks the construction-binding node (Python `assignment` of a call, TS `variable_declarator` of a `new_expression`), with `@call.construct.var` (the bound variable's identifier) and `@call.construct.class` (the constructed class's identifier). The generic adapter builds a per-enclosing-function `var -> same-file class` table from these captures, then resolves a member call's `@call.receiver` against it: a receiver bound to a same-file class that declares `@call.method` yields a `Calls` edge with confidence `Exact` (D-062e); otherwise the member call resolves through the import/alias path (D-062c) or drops. A construct binding with no enclosing derived function contributes nothing -- module-level constructions feed module-level calls, which drop anyway (D-062a).
(c) Confidence labels, attribution (D-062a/b), and the import/alias resolution of the receiver remain engine behavior unchanged (§8.2--§8.4); the captures only relocate the *shape recognition* from per-language Rust into the grammar-aware query.
Rationale: the method-call rule and the alias.name import rule both need the callee's *parts*, not its text; splitting a captured callee string on a separator would reintroduce the per-language fragility (chains, `this`, computed members) that captures exist to push into the query. This mirrors §8.6.3's touch vocabulary, which already decomposes `@touch.receiver`/`@touch.call_function` rather than handing the adapter a node to take apart.
Affects: §8.6.3, lore_derive (T8), roadmap T8.
