# @lore
# kind: state
# purpose: "Append-only record of every money movement"
ledger = []

# @lore
# kind: state
# purpose: "Current available funds"
balances = {}

# @lore
# kind: event
# name: Settled
# purpose: "Funds have moved"
SETTLED = "settled"

# @lore
# purpose: "Charge a customer"
# reads: Payment.balances
# emits: Payment.Settled
# unknown: "Concurrent charge + refund untested"
def charge():
    pass

# @lore
# on: Payment.Settled
# affects: Payment.ledger
def audit():
    pass
