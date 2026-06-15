from helpers import run
from nonexistent import ghost


# @lore
# kind: state
# name: counter
counter = 0


# @lore
# purpose: "writes and reads the state symbol"
def bump():
    global counter
    counter = counter + 1


# @lore
# purpose: "reads the state symbol — a non-write occurrence"
def show():
    return counter


# @lore
# purpose: "exact same-file call, resolved import call, and a dropped call"
def driver():
    bump()
    run()
    ghost()
