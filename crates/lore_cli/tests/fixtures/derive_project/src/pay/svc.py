# @lore
# kind: state
# purpose: "Append-only ledger"
ledger = []

# @lore
# purpose: "Charge a customer"
# affects: Payment.ledger
def charge(user):
    ledger.append(user)


def refund(user):
    charge(user)
