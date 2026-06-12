# Lore Roadmap

**Version:** 2.0. Supersedes the v1 milestone list (T1-T6 / L1-L7).
Each milestone lists: scope, out of scope, exit criteria (binding -- a milestone is done when its criteria pass in CI, not before), and the spec sections it implements. Order is mandatory (G-2). No calendar dates: the sequence and the gates are the schedule.

---

## Phase 1 -- The Grounded Tool

### T1 -- Scan and Bind
**Scope:** Workspace + `lore_intent` skeleton (types only), `lore_annotations` (scanner + tree-sitter binder for Python and TypeScript), `lore_cli` with `lore init` and `lore scan`. Manifest parsing (`lore.toml`: `[project]`, `[modules]`), module scoping rules (spec §7.5).
**Out:** clause parsing (blocks are captured as raw text), derivation, queries.
**Exit criteria:**
- `lore scan --json` on the fixture trees emits the exact expected block set: file, line span, subject identifier, qname, kind.
- Fixture suite covers per spec §7.4: decorated Python functions, exported TS declarations, multi-target assignment (`E0104`), unbound block (`E0102`), overlapping globs (`E0103`), step outside workflow (`E0105`), orphan file (`W0208`).
- `lore init` on a bare Python repo produces a valid manifest.
**Spec:** §7, §11, §12 (init/scan).

### T2 -- Intent Parse
**Scope:** `lore_intent` clause parser: full clause grammar, spans, multi-line strings, `Intent` struct exactly per §13. Unknown clause `E0202` with suggestion; duplicate singular clause `E0206`.
**Exit criteria:**
- Table-driven tests: clause text → exact `Intent` (including spans) for every clause; every E020x parse diagnostic has a test asserting its code and message shape.
- Every annotation block in the spec's own examples parses (G-13: spec examples enter CI here).
**Spec:** §3, §5, §13.

### T3 -- Graph and Structural Lint
**Scope:** `lore_graph`: node table, both adjacency maps, resolution (§6.3), applicability matrix, `depends_on` surface checks, hygiene checks (`W0210`-`W0212`), duplicate qnames. `lore lint` (structural subset) with exit codes and `--json`. `enforcement: strict` promotion. **Dogfooding starts: annotate the Lore repo, lint in Lore's CI (G-12; requires enabling the Rust row of §7.4 -- scanner+binder only).**
**Exit criteria:**
- Given fixture node/edge sets, lint emits the exact expected findings (code + qname + span), in deterministic order.
- `E0306` message names the unresolved ref and its nearest existing qname; `E0307` names both kinds.
- Lore's own CI runs `lore lint` on the Lore repo, green.
**Spec:** §6.1-§6.3, §18 bands 02x/03x (structural part).

### T4 -- `lore ask`
**Scope:** Query parser (§10.1), engine primitives `select`/`traverse` (§10.6), all query forms and filters over **declared** edges (derived edges arrive at T6 and plug into the same traversals), `show(X)`, human output format, `--json` schema, `path/reaches` with witness chains.
**Exit criteria:**
- Every query form in §10.1 has tests asserting exact result sets on a fixture graph that includes an event hop and a workflow (traversal matrix §6.4 verified literally, including the `affects*` event-hop case).
- `show` renders the full node card for any declared node (derived-only nodes are exercised at T6, when derivation lands).
- Query over a synthetic 5,000-node graph: <50 ms (§10.7), measured in CI.
**Spec:** §10, §6.4.

### T5 -- CI Surface and History
**Scope:** `lore lint` hardening for CI adoption: severity overrides (`[lint]`), `[policy]` promotion for `unknown`, stable output ordering, `--quiet`. `lore history <qname>` (git log -L). CODEOWNERS cross-check `W0207`.
**Exit criteria:**
- A GitHub Actions example workflow in the repo runs lint on a sample project and fails on a seeded `E0201`.
- `lore history` output verified against a fixture repo with scripted commits.
**Spec:** §9.3, §11 policy/lint tables, §12.

### T6 -- The Derived Layer (Python, TypeScript)
**Scope:** `lore_derive` per §8: derived nodes, `Calls` edges with Exact/Resolved resolution and drop-rule, state-touch `Affects`/`Reads` with mutator tables, confidence labeling end to end (queries now traverse `Calls`; outputs print confidence). Parse cache (G-9). Open-world resolution (D-017) switched on.
**Exit criteria:**
- Fixture suites per language assert exact derived edge sets **including required absences** (calls that must be dropped -- G-7 negative tests).
- `lore ask` call-graph queries return correct results on a fixture repo with **zero annotations**; `affects(X)` is exercised with only X's own state annotation (state touches presuppose declared State nodes, D-060).
- `lore stats` reports `unresolved_calls` and per-kind node counts.
- Full pipeline on a 20k-LOC OSS Python repo completes < 10 s cold, < 1 s warm cache.
**Spec:** §8, §10.7.

### T7 -- Reconciliation and Staleness (the trust milestone)
**Scope:** Claim statuses per §9.1 (four-status algorithm, symbol-occurrence test), `W0302/E0302`, `W0303` behind policy, staleness `W0301` via git blame (§9.2), `lore stats` claim-status breakdown. All statuses visible in `ask`/`show` output.
**Exit criteria:**
- Table-driven reconciliation tests cover all four statuses for `Affects`, `Reads`, `Triggers`, including the Heuristic-absence guard (G-7).
- **Seeded-drift test:** a scripted fixture repo where 10 annotations are made false in 10 different ways (effect removed, function gutted, state renamed, ...); lint flags ≥ 8, with zero false `Contradicted` on the 20 true annotations. This test is permanent regression armor.
- Staleness verified against a fixture repo with scripted commit timestamps; clean skip outside git.
**Spec:** §9.

### T8 -- Language Packs, Remaining Languages, and Export
**Scope:** Language-pack loader and generic adapter (§8.6, D-070): pack manifest validation (band `E041x`), tier semantics (scan/bind/derive), builtin packs embedded at build time. Built-in import-strategy library (`relative`, `root_relative`, `package_dir`, `manifest_prefix`) with the named-custom escape hatch (D-071). Conformance harness running every builtin pack's fixture suite through the real scan→bind→derive pipeline in CI. Python and TypeScript migrate onto packs (their existing fixtures become their conformance suites). **Go and Java land as the first pure packs** (zero per-language Rust); **Rust lands as a pack with one custom import strategy** (`rust_use_paths`, D-071c) -- all three completing their §7.4 + §8.5 rows (scanner, binder, derivation). `lore graph --dot` with `--focus/--depth`.
**Out:** WASM grammars and external (non-embedded) packs -- the pack format reserves their key space (§8.6.1); accepting them is a later milestone via a new D-entry.
**Exit criteria:**
- Pack-loader unhappy path first (G-11): each `E041x` class is produced by a malformed fixture pack (unknown key, tier/artifact mismatch, unknown format version, `wasm` grammar, unknown strategy, missing mandatory fixture class) with exact code + message tests.
- Python/TS pre-T8 test suites pass unchanged after the migration to packs -- zero behavioral drift.
- Go and Java: per-language fixture parity with the T1/T6 suites, expressed as conformance fixtures including required absences (dropped calls, non-writes -- G-7 negative tests); both packs contain zero per-language Rust.
- Rust: pack loads with `rust_use_paths`; Lore's own repo (Rust) now reconciles, not just lints structurally.
- The conformance harness runs every builtin pack's suite in CI; a seeded failing fixture refuses the pack with `E0415`, proven by a harness test.
- DOT output renders under `dot -Tsvg` without warnings.
**Spec:** §7.4, §8.5, §8.6, §12.

### T9 -- MCP Server
**Scope:** `lore mcp` (stdio): tools `lore_ask`, `lore_show`, `lore_lint`, `lore_history`, returning the §10.4 JSON. Read-only by construction.
**Exit criteria:** a scripted MCP client session exercises all four tools against a fixture project; tool descriptions are good enough that an agent picks the right tool for "what writes to the ledger?" in an eval transcript checked into the repo.
**Spec:** §12 (D-037).

### T10 -- Thesis Validation (the Gate, D-039)
**Scope:** Apply the tool end to end to (a) one external OSS repo ≥ 20k LOC and (b) one internal real project, annotating ≥ 30 subjects across ≥ 3 modules in each. Write up findings as `validation-report.md` (the one permitted retrospective doc).
**Exit criteria -- ALL must pass; Phase 2 is blocked otherwise:**
1. Five canonical onboarding questions per repo ("what writes to <state>?", "what is <node> for and why?", "what reaches <state> from a public entry point?", "what is unresolved in <service>?", "who owns <area>?") answered correctly by `lore ask`/`show`, verified manually against the code.
2. The T7 seeded-drift protocol repeated on the real repo: ≥ 8/10 planted lies caught, 0 false `Contradicted`.
3. Cold run < 30 s, warm queries < 200 ms on the larger repo.
4. Annotating 30 subjects took < 2 hours of developer time (measured) -- the cost-of-intent claim.
5. At least one genuine, previously unknown finding surfaced (an honest `unknown`, an undocumented effect, a stale claim) -- recorded in the report.
**Failing the gate** means iterating within Phase 1 (new D-entries for what must change), not proceeding on hope.

---

## Phase 2 -- The Language (opens only after T10)

### L1 -- Lexer + Parser
Full grammar of spec §15 including NEWLINE rules (D-034), `with`, `?`, `emit`, error unions. Exit: every `.lore` example in the spec parses to the exact expected AST; every E-band parse error fixture produces its code; malformed-input fuzzing (1h, no panics). Unhappy path first (G-11): error-recovery tests precede AST-shape tests in the commit history.

### L2 -- Semantic Analysis
Rules §16.1-2 and 6-7: applicability, resolution via the unchanged `lore_graph`, exhaustive match, intent-graph construction from `.lore` source -- `lore ask` works on `.lore` projects from this milestone. Exit: the Phase 1 query test suite passes verbatim against a `.lore` fixture project; `E0201/E0203/E0204/E0205` parity with Phase 1.

### L3 -- Type Checking + Bytecode
Rules §16.3-5 + §16's emit rule (tags `E0503`, sealed `E0504`, error unions `E0505/E0506`, state permissions `E0507/E0508`, `E0512`). Bytecode per §17.2-3 with the versioned header (D-035); `lorec build` emits `.lorec`; disassembler for tests. Intent-preserving opcodes are in from this milestone -- not retrofittable (G-8). Any opcode-set change here requires a D-entry. Exit: golden-disassembly tests per construct; a bad-version file is rejected with a clean error; `E0507` fires when `affects` is removed from the canonical example.

### L4 -- VM Core
Fetch/decode/execute; calls, locals, arithmetic, match, records, `with`, raise-as-value, `TRY_PROPAGATE`, state API over in-memory backing (D-031). Exit: execute-and-assert tests per opcode group; the §19 `charge` function (with `Gateway` stubbed) runs end to end and both its state writes are observable; type-mismatch and stack-underflow produce E06xx diagnostics, not panics (G-11).

### L5 -- Services, Externals, Events
`ROUTE_REGISTER` + HTTP serving; `lore.runtime.toml`; `EXTERN_CALL` against the HostFn table with launch-time bind check (D-033); event dispatcher per D-032 (post-Ok release, FIFO per type, handler-failure counters). Exit: integration test boots `PaymentService`, drives `POST /charge` over real HTTP, asserts ledger/balances state, asserts the `PaymentSettled` handler ran after Ok and did NOT run on an `Err` path; unbound external fails at launch with the binding name in the message.

### L6 -- `lore trace`
Live-VM query per §17.6 (which gets fully specified -- via D-entries -- at the start of this milestone, not before). Exit: against the L5 integration service, trace reports active affects regions, unknown-path hit counts, and event counters matching scripted traffic.

### L7 -- Full Pipeline Proof
Write a real multi-module service in Lore end to end (the §19 example grown to 3+ modules, a workflow, and an external), run it, query it statically and live. Exit: the service handles a scripted load run; `lore ask path(<entrypoint>, Payment.ledger)` and `lore trace` agree with observed behavior; a written L7 report mirrors the T10 protocol. **Optimization is unlocked only after this milestone (G-9).**

---

## Risks (ordered by expected damage)

1. **Derivation precision (T6).** False derived edges destroy trust faster than missing ones (G-7). Mitigation is structural: the drop-rule, confidence labels, and negative-fixture tests are all exit criteria, not aspirations. If a language's heuristics can't meet the seeded-drift false-positive bar, ship it scanner+binder-only and say so in `lore stats`.
2. **T10 fails on cost-of-intent (criterion 4).** If annotation takes too long, the fix is ergonomics inside Phase 1 (better `E` messages, `lore init` suggesting annotation stubs for high-churn modules), never weakening the gate.
3. **Scope creep into Phase 2 early.** The gate (D-039) plus G-2 are the defense; any Phase 2 commit before the gate is a review-blocking violation.
4. **tree-sitter grammar churn.** Pin grammar crate versions in the workspace; fixture suites catch breakage on upgrade.
5. **Composition pressure on the query language.** Expected around month 2-3 of real use. The §10.6 engine shape is the pre-laid escape hatch; resist ad-hoc query forms (each new form needs a D-entry showing it's sugar over select/traverse).
