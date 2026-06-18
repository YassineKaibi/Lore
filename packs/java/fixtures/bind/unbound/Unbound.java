class Unbound {
  void m() {
    // @veridikt
    // purpose: "this block binds to a statement, not a declaration — E0102"
    compute();
  }
}
