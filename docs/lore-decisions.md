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
