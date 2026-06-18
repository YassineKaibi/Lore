# @veridikt
# purpose: "a function subject (@subject.function)"
def alpha():
    pass


# @veridikt
# purpose: "a class subject (@subject.type)"
class Beta:
    pass


# @veridikt
# kind: state
# name: gamma
gamma = 0


# @veridikt
# purpose: "decorated function — exercises the decorated_definition wrapper descent"
@decorator
def delta():
    pass
