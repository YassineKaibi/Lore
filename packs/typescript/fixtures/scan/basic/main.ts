// A plain line comment that must NOT scan as a block (no @lore marker).

/* A block comment mentioning @lore must NOT scan as a block (§7.1: only the
   line comment token carries annotations). */

// @lore
// purpose: "the one block that scans"
function widget() {
  return 1;
}
