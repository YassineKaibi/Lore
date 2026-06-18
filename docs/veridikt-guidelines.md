# Veridikt Project Guidelines

**Version:** 2.0. Supersedes v1 entirely.
Rules are numbered G-1..G-14 so spec, roadmap, and reviews can cite them. Every implementation decision should trace to one of these or to a `veridikt-decisions.md` entry.

---

## G-1. The Spec is the Source of Truth, and the Ledger is How It Changes

Every implementation decision must trace to `veridikt-spec.md`. Code that contradicts the spec is a bug in the code. When implementation reveals a spec gap or contradiction, you do not improvise in code and you do not edit history: you add a `D-NNN` entry to `veridikt-decisions.md` (question, decision, rationale, what it affects), update the spec to match, and only then write the code. The ledger is append-only; superseding a decision means a new entry that names the old one. "I'll fix the spec later" is how implementations silently fork from their design -- it is banned.

## G-2. One Crate at a Time, in Order, with the Gate

```
Phase 1:  veridikt_intent -> veridikt_annotations -> veridikt_graph -> veridikt_cli
                                          -> veridikt_derive -> (reconciliation in veridikt_graph)
Phase 2:  veridikt_lexer -> veridikt_parser -> veridikt_semantic -> veridikt_bytecode -> veridikt_vm
          (reusing veridikt_intent + veridikt_graph unchanged)
```

Never start a crate before the previous one has a working, tested core. Never start any Phase 2 crate before the T10 gate (roadmap) passes -- the gate is numeric, not a feeling (D-039). The temptation to jump to the language before the tool has proven the thesis is the project's single largest schedule risk; the gate exists to make jumping ahead a visible, deliberate violation rather than a drift.

## G-3. The Shared Contracts are Sacred

Spec §13 (`Intent`, `IntentNode`, `Edge`, `Graph`) is consumed by every crate in both phases. Before changing any field: grep every use across all crates, write the migration in the PR description, and update spec §13 in the same commit. The Phase 2 `.veridikt` AST in `veridikt_parser` is the second contract and gets the same treatment from L1 onward. Contract changes late are the most expensive change class in this project -- get node shapes right early, and when they must move, move them loudly.

## G-4. Test at the Boundary, Not the Implementation

Each crate has one public interface; tests assert that interface:

- `veridikt_annotations`: source tree in → exact set of blocks, subjects, qnames out (snapshot tests over fixture trees per language).
- `veridikt_intent`: clause text in → exact `Intent` AST out, including spans.
- `veridikt_derive`: source tree in → exact derived node/edge set with confidences out. Fixtures MUST include negative cases: calls that must be *dropped*, not guessed (G-7).
- `veridikt_graph`: nodes+edges in → exact findings, claim statuses, and query results out. Reconciliation tests are table-driven over the four-status decision in spec §9.1.
- `veridikt_cli`: command in → exact stdout (human and `--json`) and exit code out.
- Phase 2 crates: token stream / AST shape / errors / disassembly / output+side-effects respectively, as in v1.

If an internal function is hard to test through the boundary, the abstraction is wrong -- fix the boundary, don't export the internals.

## G-5. Error Messages are a Feature; the Registry is the Law

Every diagnostic carries a code from spec §18, states what went wrong in plain language, where (file:line), and what to do. Write the message before the code that produces it; if you cannot write a clear message, you do not understand the failure well enough to implement it. New diagnostics take the next free number in their band and are added to §18 in the same PR. No stringly ad-hoc errors anywhere -- the registry is also the test surface (lint tests assert codes, not prose).

## G-6. Incremental Milestones, Each One Useful

Milestones and exit criteria live in `veridikt-roadmap.md` and are binding. Never be in a state where nothing runs. If you are between milestones and stalled, the milestone is too large -- split it in the roadmap (with a decisions entry if scope moves), don't grind.

## G-7. Never Present a Guess as a Fact

This is the project's distinctive rule. The product's entire value is that its answers can be trusted, so:

- Every derived edge carries `Exact | Resolved | Heuristic` and every surface prints it.
- Unresolvable calls are dropped and counted, never guessed (spec §8.2 rule 3).
- `Heuristic` absence alone never yields `Contradicted` -- only the independent symbol-occurrence test does (spec §9.1).
- Declared edges always show their claim status. `Unverifiable` is an honest answer; print it.

If a feature forces a choice between coverage and certainty, choose certainty and surface the gap in `veridikt stats`. A wrong edge poisons trust in every correct one.

## G-8. Load-Bearing Decisions vs Deferrable Ones

| Decision | Load-bearing? | Status |
|---|---|---|
| In-source intent (no sidecars) | Yes -- intent must not drift from code | Never drop |
| Derived layer + reconciliation | Yes -- it IS the trust thesis | Never drop (D-001) |
| `veridikt ask` + `show` | Yes -- proves the thesis | By T4 |
| Confidence labeling | Yes -- G-7 | From T6 |
| Staleness detection | Yes -- answers the #1 objection | By T7 |
| Qualified references, no implicit context | Yes | Never drop |
| Intent enforcement via state API | Yes -- core of Phase 2 | From L2 |
| Intent-preserving opcodes | Yes -- enables `veridikt trace` | From L3, not retrofittable |
| `sealed` types | No | Deferrable |
| MCP server | No (high-value, not thesis) | T9, droppable under pressure |
| DOT export, `veridikt stats` polish | No | Deferrable |
| State persistence backends, dynamic FFI loading | No | Post-L7 |

When implementation gets hard, check this table before simplifying. Dropping a "Never drop" row requires a decisions entry and is presumed wrong.

## G-9. No Premature Optimization, Two Exceptions

The first VM will be slow; that is correct -- optimize after L7. The exceptions, both Phase 1 and both because query latency is product-visible from T4: (1) the graph structure (both adjacency directions in memory, spec §10.7); (2) the parse cache (`.veridikt-cache/` content-hash cache of tree-sitter parses), because re-parsing a 20k-LOC repo per query would make the tool feel broken during T10 validation. Nothing else gets optimized before it is measured.

## G-10. Commit Messages are `because` Annotations for the Project

The diff shows what; the message explains why. This is doubly binding here because `veridikt history` renders these messages back to users as the project's change-intent record -- the tool dogfoods your commit discipline.

```
// Bad
fix binder edge case

// Good
binder: bind through Python decorated_definition

decorators sit between the @veridikt block and the function node, so the
binder must descend wrapper nodes (spec 7.4) instead of failing with
E0102 -- found while annotating the flask fixture
```

## G-11. Build the Unhappy Path First

For every feature, the failure case before the success case: the scanner handles a malformed block before a perfect one; the binder produces E0102 before it binds; resolution reports E0306/E0307 before it builds edges; reconciliation produces Contradicted before Verified; the Phase 2 parser produces good errors before good ASTs; the VM handles a bad bytecode version before executing a good file. Failure paths reveal design problems success paths hide -- and in this project the failure paths (lint findings) are the product.

## G-12. Dogfood from T3 Onward

From the moment `veridikt lint` runs, the Veridikt repository itself carries `@veridikt` annotations (Rust is a supported language for exactly this reason, D-014) and `veridikt lint` runs in Veridikt's own CI. Every annoyance you feel annotating your own code is a finding about the product; file it. A team that won't annotate its own repo has learned something important about its thesis.

## G-13. Two Audiences, One Document Set

These documents are the complete implementation context for both a human developer and an AI coding assistant. Consequences: no knowledge may live only in chat threads or heads -- if it isn't in the spec, ledger, or roadmap, it doesn't exist; all normative material uses MUST/SHOULD; examples in the spec must actually conform to the grammar (treat spec examples as test fixtures -- T2 onward, parse them in CI); and ambiguity discovered by either audience is a documentation bug to fix via G-1, not a judgment call to make locally.

## G-14. The Three Questions

Before writing any code:
1. Is this in the spec (or in a decisions entry)?
2. Does it strengthen the intent graph -- more nodes, more edges, cheaper declarations, or more trustworthy answers?
3. Am I in the right crate for this concern?

If all three are yes, build it. If question 2's honest answer is "it's just convenient," it faces the high bar from the spec's governing principle -- default no.
