package app

// A plain line comment that must NOT scan as a block (no @veridikt marker).

/* A block comment mentioning @veridikt must NOT scan as a block (§7.1). */

// @veridikt
// purpose: "the one block that scans"
func Widget() int {
	return 1
}
