# @veridikt
# kind: state
# purpose: "Ledger"
ledger = []

# @veridikt
# kind: state
# name: balances
balances = {}

# @veridikt
# purpose: "Charge a customer"
# affects: Payment.ledgr
# reads: Payment.balances
# triggers: User.notify
def charge():
    pass
