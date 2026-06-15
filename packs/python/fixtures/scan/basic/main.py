# A plain comment that must NOT scan as a block (no @lore marker, §7.1).
# @lore is only recognized on the comment token; this line alone is prose.


# @lore
# purpose: "the one block that scans"
def widget():
    """A docstring mentioning @lore must not scan as a block either."""
    return 1
