// A plain line comment that must NOT scan as a block (no @veridikt marker).

/* A block comment mentioning @veridikt must NOT scan as a block (§7.1). */

class Main {
  // @veridikt
  // purpose: "the one block that scans"
  int widget() {
    return 1;
  }
}
