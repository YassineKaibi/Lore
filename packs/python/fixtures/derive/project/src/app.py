from helpers import run
from nonexistent import ghost


# @veridikt
# kind: state
# name: counter
counter = 0


# @veridikt
# purpose: "writes and reads the state symbol"
def bump():
    global counter
    counter = counter + 1


# @veridikt
# purpose: "reads the state symbol — a non-write occurrence"
def show():
    return counter


# @veridikt
# purpose: "exact same-file call, resolved import call, and a dropped call"
def driver():
    bump()
    run()
    ghost()
