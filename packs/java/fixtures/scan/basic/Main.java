// A plain line comment that must NOT scan as a block (no @lore marker).

/* A block comment mentioning @lore must NOT scan as a block (§7.1). */

class Main {
  // @lore
  // purpose: "the one block that scans"
  int widget() {
    return 1;
  }
}
