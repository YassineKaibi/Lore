# @lore
# kind: state
# purpose: "Unpaid invoices awaiting settlement"
invoices = []


# @lore
# purpose: "Record a new invoice"
# affects: Billing.invoices
def add_invoice(invoice):
    invoices.append(invoice)
