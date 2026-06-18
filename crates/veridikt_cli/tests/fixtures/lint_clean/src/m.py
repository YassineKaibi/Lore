# @veridikt
# kind: state
# purpose: "Request counter"
count = 0

# @veridikt
# purpose: "Bump the counter"
# affects: App.count
def bump():
    count += 1
