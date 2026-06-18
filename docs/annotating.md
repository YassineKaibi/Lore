# Annotating a project with `@veridikt` (authoring quickref)

**Non-normative.** This is a distilled, agent-facing guide to *writing* `@veridikt`
annotation blocks. The law is `veridikt-spec.md` §3 (clauses), §5 (clause
semantics), §7 (blocks, binding, scoping); on any conflict, the spec wins.
Use this to get annotating fast, then let `veridikt lint` correct you.

---

## What you are doing

Veridikt keeps two layers over the code: a **derived** layer (facts extracted
from source — nodes, calls, structure — true by construction) and a
**declared** layer you write as `@veridikt` blocks carrying *human intent the code
cannot reveal*: what something is *for*, *why* it is the way it is, what it is
*meant* to affect, who owns it, what is *unknown*. `veridikt lint` then
**reconciles** every effect claim against the code and labels it `Verified` /
`Unverified` / `Contradicted` / `Unverifiable`. So: write honest intent; the
tool checks it.

Annotate the **load-bearing** subjects first — public entry points, state,
events, the functions whose effects matter. You do not need to annotate
everything; unannotated code still appears in the derived graph.

---

## 1. Block anatomy

A block is a run of **contiguous line comments** whose first content line is
exactly `@veridikt`, placed **immediately above** the declaration it describes
(blank lines between block and declaration are allowed). Only **line**
comments are scanned — never block comments or docstrings (`/* */`, `""" """`).

> ⚠️ The `@veridikt` line must be the **first** line of its comment run. A doc
> comment glued directly above it (`/// …` then `// @veridikt`, no blank line)
> makes the run start at the doc text, and the block is **not** recognized —
> you'll get `W0110`. Put a blank line between any preceding comment and
> `// @veridikt`.

```
<comment-token> @veridikt
<comment-token> kind: <node-kind>          # optional; default is "function"
<comment-token> name: <Ident>              # optional; overrides the declared name
<comment-token> <clause>: <value>
<comment-token> ...
<the declaration>
```

Comment token per language: Python `#`; TypeScript/JS, Go, Java, Rust `//`.

- `kind:` ∈ `module | service | workflow | step | state | event | type |
  error | function | external`. Omit it for a plain function.
- `name:` sets the node's local name. Required when the declaration has no
  single obvious identifier (multi-target assignment, destructuring) — else
  `E0104`. A quoted string value may span several comment lines.
- Each node's full name is **`<Module>.<name>`** (see §3).

```python
# @veridikt
# kind: state
# purpose: "Append-only record of every money movement"
ledger = []
```

```rust
// @veridikt
// purpose: "Charge a customer and append the movement to the ledger"
// because: "The caller supplies the idempotency key; we do not deduplicate here"
// affects: Payment.ledger
// reads: Payment.balances
fn charge(user_id: UserId, amount: Money) -> Result<Receipt, ChargeError> { ... }
```

---

## 2. The clauses

| Clause | Value | Means | Notes |
|---|---|---|---|
| `purpose` | `"string"` | what the construct exists for | once |
| `owner` | `"string"` | responsible team | once; cross-checked vs CODEOWNERS |
| `because` | `"string"` | why a non-obvious decision was made | repeatable |
| `unknown` | `"string"` | what is unresolved, untested, not understood | repeatable; surfaced by lint (`W0213`) |
| `assumes` | `"string"` | what must hold of inputs/environment on entry | repeatable |
| `affects` | `ref, ...` | state this **writes** | refs → `State`; reconciled |
| `reads` | `ref, ...` | state this **reads** | refs → `State`; reconciled |
| `triggers` | `ref, ...` | cross-module synchronous calls it makes | refs → `Function`; reconciled vs the call graph |
| `emits` | `ref, ...` | events it publishes | refs → `Event`; `Unverifiable` in Phase 1 |
| `on` | `ref, ...` | events it handles | refs → `Event` |
| `depends_on` | `ref, ...` | declared dependency surface | refs → `Module`/`Service`/`External` |
| `route` | `METHOD "path"` or `"path"` | HTTP entry point | bare = service base path; method form marks a handler |
| `enforcement` | `strict` or `warn` | module policy (findings inside become errors under `strict`) | on modules only |

A **ref** is a fully qualified dotted name: `Payment.ledger`. There is **no
implicit context** — always write the whole `Module.name`. Strings use `\"`
and `\\` for an embedded quote/backslash.

---

## 3. Names, modules, and scoping

A subject's full name is `<Module>.<localname>`. A file gets its module one of
two ways:

1. **`veridikt.toml [modules]` globs** (the usual way), e.g. `"src/sim/**" = "Sim"`.
   Run `veridikt init` first; it proposes these from your directory names. (Make
   the globs **disjoint** — overlap is `E0103`. Glob `*` matches within one
   path segment; use `**` to cross `/`, so `src/*` and `src/sim/**` are
   disjoint.)
2. **A top-of-file scoping block** — a `kind: module` (or `service` /
   `workflow`) block at the top of the file. Scoping blocks bind to no
   declaration and **must** carry an explicit `name:` (`E0108`).

A file mapped by neither → its annotated subjects become `_orphan.<name>`
with `W0208`.

**Module granularity is a modelling choice.** `triggers` edges are
*cross-module only* (an intra-module call is `W0205`/`W0302`), so the coarser
your modules, the more call edges fall *inside* a module and become
undeclarable. On a cohesive codebase where each file is a single
responsibility, mapping per file (or per small directory) — rather than one
module per top-level dir — makes those dominant call edges declarable as
`triggers`. `veridikt init`'s directory-granularity default is the safe starting
point, not a ceiling; tighten it when the call graph you want to declare lives
within one proposed module.

```python
# @veridikt
# kind: module
# name: Payment
# purpose: "Money-movement primitives"
# owner: "payments-team"
```

**Workflows/steps:** a `step` needs an enclosing `kind: workflow` block in the
same file, and steps are ordered by appearance (`E0105` otherwise). A step's
name is `<Workflow>.<step name>`, and every step must declare at least one of
`triggers` / `emits` / `on` (`E0204`).

---

## 4. Which clauses are legal on which kind (applicability)

**R** required · **Rec** recommended (`W0209` if absent) · **O** optional ·
**—** illegal (`E0203`). Required clauses are enforced by `veridikt lint`
(`E0201`).

| Clause | Module | Service | Workflow | Step | State | Event | Type | Error | Function | External |
|---|---|---|---|---|---|---|---|---|---|---|
| `purpose` | R | R | R | O | Rec | R | O | — | O | R |
| `owner` | R | R | R | — | inherit | inherit | — | — | O | R |
| `because` | O | O | O | O | O | O | O | R* | O | O |
| `unknown` | O | O | O | O | O | O | O | — | O | O |
| `assumes` | — | — | O | O | — | — | — | — | O | O |
| `affects` | — | — | — | O | — | — | — | — | O | — |
| `reads` | — | — | — | O | — | — | — | — | O | — |
| `triggers` | — | — | — | O | — | — | — | — | O | — |
| `emits` | — | — | — | O | — | — | — | — | O | — |
| `on` | — | — | — | O | — | — | — | — | O | — |
| `depends_on` | O | O | O | — | — | — | — | — | — | — |
| `route` | — | R (base) | — | — | — | — | — | — | O (in a service) | — |
| `enforcement` | O | — | — | — | — | — | — | — | — | — |

- `owner` on State/Event is **inherited** from the owning module — do not
  declare it locally (`E0203`).
- `route` on a function is legal only when its parent is a `Service` (`E0205`).
- `*` On an `Error`, `because` is a required *field* of the error definition,
  not an intent clause.
- These requirements apply only to subjects you declare with a block; nodes
  that exist only via the derived layer or a `veridikt.toml` mapping are exempt.

---

## 5. The authoring loop (let the tool teach you)

You do not have to get it perfect from memory. Annotate, then:

```sh
veridikt lint            # reports every problem with a code, location, and fix
```

The findings are the teacher — fix them and re-run:

| Code | Meaning | Fix |
|---|---|---|
| `E0201` | missing required intent | add the required clause for that kind (§4) |
| `E0202` | unknown clause name | use the suggested clause it names |
| `E0203` | clause illegal for this kind | remove it (or change the kind) |
| `E0204` | step declares no effect | add a `triggers`/`emits`/`on` |
| `E0205` | `route` outside a service | only put `route` on service functions |
| `E0206` | duplicate singular clause | keep one `purpose`/`owner`/... |
| `E0207` | malformed clause value | fix the syntax (quotes, ref shape) |
| `E0306` | ref resolves to no node | fix the qualified name (it names the nearest) |
| `E0307` | ref points at the wrong kind | point `affects` at a `State`, `triggers` at a `Function`, etc. |
| `E0103` | overlapping module globs | make `veridikt.toml [modules]` disjoint |
| `W0110` | `@veridikt` glued under another comment | add a blank line between the preceding comment (e.g. a `///` doc) and `// @veridikt` |
| `W0205` | `triggers` to the same module | `triggers` is for *cross-module* calls |
| `W0206` | unused `depends_on` | remove it or actually use the dependency |
| `W0208` | orphan file | map the file in `veridikt.toml [modules]` |
| `W0302` | **contradicted** claim | the code does not do what you declared — fix the claim or the code |

`W0302` is the point of the whole exercise: it means your declared `affects`/
`reads`/`triggers` does not match what the code actually does.

---

## 6. Use the derived graph while you write effect claims

If the veridikt MCP server (or `veridikt ask`) is available on the project, query the
**derived** layer to ground your `affects`/`reads`/`triggers` in what the code
really does before you declare them:

- `touches(<fn>)` — the state a function already writes/reads (derived).
- `callees(<fn>)` — the calls `<fn>` **makes**; this is what a `triggers:`
  clause declares, so query it to ground one. (`callers(<fn>)` is the
  opposite direction — who calls `<fn>` — and `reaches(<fn>)` is everything
  `<fn>` transitively reaches, not just its calls.)

Declare what you *intend*; reconciliation then confirms it against the code.
(State effects only show up once the target has a `kind: state` block — the
derived layer cannot infer that a variable *is* state on its own.)

---

## 7. Honesty rules (the product's whole point)

- Never write a claim you have not confirmed against the code. A wrong
  `affects` is worse than a missing one.
- When something is genuinely unresolved or untested, say so with
  `unknown:` — it is a first-class, honest answer, not a failure.
- Refs must resolve to a real node of the right kind. If unsure of a name,
  `veridikt lint` (or `veridikt ask show(<name>)`) will tell you the nearest match.
- Prefer fewer, true, load-bearing annotations over broad, shallow coverage.
