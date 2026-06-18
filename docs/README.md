# Veridikt Documentation

Veridikt maintains a single **intent graph with two layers** over a codebase and continuously checks one layer against the other: the *derived* layer is what the code does (extracted statically, true by construction), the *declared* layer is why it does it (`@veridikt` blocks in comments), and *reconciliation* compares the two and labels every human claim `Verified`, `Unverified`, `Contradicted`, or `Unverifiable`. (Spec §1.)

This is the index to the layer documentation. For the project pitch, install instructions, and the canonical quickstart, see the [root README](../README.md). The authoritative behavior is defined in [`veridikt-spec.md`](veridikt-spec.md); the documents below describe its surface.

## The three layers

The layers map to a strict crate dependency order (no crate reaches backwards - G-2/G-3):

```
veridikt_intent -> veridikt_annotations -> veridikt_derive -> veridikt_graph -> veridikt_cli
```

| Doc | Layer | Crate | Read it to |
|---|---|---|---|
| [Derived facts](derived-facts/README.md) | The *what* (§8) | `veridikt_derive` | Extract and query facts with zero annotations |
| [Declared intent](declared-intent/README.md) | The *why* (§5, §7) | `veridikt_annotations` | Write intent as `@veridikt` blocks in comments |
| [Reconciliation](reconciliation/README.md) | The trust (§9) | `veridikt_graph` | Catch drift between the two and fail CI on it |

End to end: **declare** intent in comments → **derive** facts from source → **reconcile** the two → **query, lint, and render** the labeled graph. Each layer doc follows the same shape - description, why, drawbacks, architecture, usage, quickstart - and states its own limitations in full.

## Specification and process

| Document | What it is |
|---|---|
| [`veridikt-spec.md`](veridikt-spec.md) | The authoritative specification - code that contradicts it is a bug in the code |
| [`veridikt-decisions.md`](veridikt-decisions.md) | Append-only decisions ledger (`D-NNN`); how the spec changes |
| [`veridikt-guidelines.md`](veridikt-guidelines.md) | Engineering rules (`G-1`..`G-14`) that reviews cite by number |
| [`veridikt-roadmap.md`](veridikt-roadmap.md) | Milestones with binding exit criteria; order is mandatory |
