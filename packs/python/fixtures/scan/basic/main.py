# A plain comment that must NOT scan as a block (no @veridikt marker, §7.1).
# @veridikt is only recognized on the comment token; this line alone is prose.


# @veridikt
# purpose: "the one block that scans"
def widget():
    """A docstring mentioning @veridikt must not scan as a block either."""
    return 1
