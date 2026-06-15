package app;

import app.util.Helper;
import app.util.Missing;

class Service {
  // @lore
  // kind: state
  // name: counter
  static int counter = 0;

  // @lore
  // purpose: "same-file call, root_relative import call, and a dropped call"
  void driver() {
    bump();
    Helper.run();
    Missing.gone();
    counter = counter + 1;
  }

  // @lore
  // purpose: "reads the state symbol — a non-write occurrence"
  int show() {
    return counter;
  }

  void bump() {}
}
