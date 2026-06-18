# T9 MCP tool-selection eval

Roadmap **T9** exit criterion: *"tool descriptions are good enough that an
agent picks the right tool for 'what writes to the ledger?' in an eval
transcript checked into the repo."*

- **Project:** `mcp_project` (this directory) — a two-state Python `Payment`
  module whose `charge` function both declares and actually performs
  `ledger.append(...)`.
- **Server:** `veridikt mcp` (stdio), advertising the four read-only tools of
  D-079b.

This transcript is **executable**: the integration test
`mcp_eval_what_writes_to_the_ledger` (in `tests/cli_mcp.rs`) replays the
agent's tool call below against this fixture through a real `veridikt mcp` session
and asserts the result, so the transcript cannot drift from the server's
behavior (D-079f).

---

## Transcript

**1. Client → Server (`tools/list`).** The server advertises four tools.
Relevant excerpt (descriptions abbreviated):

```
veridikt_ask      Query the intent graph … affects(State) — what writes a piece of
              state; reads(State) — what reads it; callers/callees/reaches —
              call reachability; … Example: "what writes to the ledger?" →
              affects(Payment.ledger).
veridikt_show     Show one node's card: kind, intent clauses, edges.
veridikt_lint     Run the full lint: reconciliation, staleness, hygiene.
veridikt_history  Git change history of a node's subject span.
```

**2. User → Agent.**

> What writes to the ledger?

**3. Agent (reasoning).** "What *writes to* a piece of state" is an effect
query. `veridikt_ask`'s description maps this phrasing directly to
`affects(State)`. The ledger is the state node `Payment.ledger`. The other
three tools do not fit: `veridikt_show` describes one named node, `veridikt_lint`
audits drift, `veridikt_history` reports git commits — none answer "what writes
to X". Select **`veridikt_ask`**.

**4. Agent → Server (`tools/call`).**

```json
{"name": "veridikt_ask", "arguments": {"query": "affects(Payment.ledger)"}}
```

**5. Server → Agent (`result.structuredContent`, abbreviated).**

```json
{
  "query": "affects(Payment.ledger)",
  "results": [
    {
      "qname": "Payment.charge",
      "kind": "Function",
      "via": [{"from": "Payment.charge", "to": "Payment.ledger",
               "edge": "Affects", "layer": "Declared", "status": "Verified"}]
    }
  ]
}
```

**6. Agent → User.**

> `Payment.charge` writes to the ledger. The claim is **Verified** — Veridikt
> checked the declared `affects: Payment.ledger` against the derived call
> graph and found the actual `ledger.append(...)` in the function body, so the
> annotation and the code agree.

---

## Why the description carries the choice

The single sentence *"affects(State) — what writes a piece of state"* in
`veridikt_ask`'s description is the load-bearing phrase: it pairs the user's verb
("writes") with the query form (`affects`) and the argument category
(`State`), which is exactly the mapping the agent must make. The `Verified`
label in the answer is what makes the reply trustworthy rather than a guess
(G-7) — the whole point of feeding an agent the intent graph instead of the
raw source.
