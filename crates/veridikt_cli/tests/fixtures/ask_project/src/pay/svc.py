# @veridikt
# kind: state
# purpose: "Append-only record of every money movement"
ledger = []

# @veridikt
# kind: state
# purpose: "Current available funds"
balances = {}

# @veridikt
# kind: event
# name: Settled
# purpose: "Funds have moved"
SETTLED = "settled"

# @veridikt
# purpose: "Charge a customer"
# reads: Payment.balances
# emits: Payment.Settled
# unknown: "Concurrent charge + refund untested"
def charge():
    pass

# @veridikt
# on: Payment.Settled
# affects: Payment.ledger
def audit():
    pass
