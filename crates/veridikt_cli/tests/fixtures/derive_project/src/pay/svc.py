# @veridikt
# kind: state
# purpose: "Append-only ledger"
ledger = []

# @veridikt
# purpose: "Charge a customer"
# affects: Payment.ledger
def charge(user):
    ledger.append(user)


def refund(user):
    charge(user)
