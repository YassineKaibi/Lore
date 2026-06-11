# @lore
# kind: state
# purpose: "Ledger"
ledger = []

# @lore
# kind: state
# name: balances
balances = {}

# @lore
# purpose: "Charge a customer"
# affects: Payment.ledgr
# reads: Payment.balances
# triggers: User.notify
def charge():
    pass
