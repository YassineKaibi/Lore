// A plain line comment that must NOT scan as a block (no @lore marker).

/* A block comment mentioning @lore must NOT scan as a block (§7.1). */

// @lore
// purpose: "the one block that scans"
fn widget() -> u32 {
    1
}
