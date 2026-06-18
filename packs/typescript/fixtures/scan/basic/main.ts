// A plain line comment that must NOT scan as a block (no @veridikt marker).

/* A block comment mentioning @veridikt must NOT scan as a block (§7.1: only the
   line comment token carries annotations). */

// @veridikt
// purpose: "the one block that scans"
function widget() {
  return 1;
}
