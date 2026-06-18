# @veridikt
# kind: state
# purpose: "Append-only record of every money movement"
ledger = []

# @veridikt
# kind: state
# purpose: "Current available funds per account"
balances = {}

# @veridikt
# purpose: "Charge a customer and append the movement to the ledger"
# because: "The caller supplies the idempotency key; we do not deduplicate here"
# affects: Payment.ledger
# reads: Payment.balances
def charge(user_id, amount):
    if balances.get(user_id, 0) < amount:
        raise ValueError("insufficient funds")
    ledger.append((user_id, amount))
    return True
