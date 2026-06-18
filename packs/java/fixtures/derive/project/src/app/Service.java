package app;

import app.util.Helper;
import app.util.Missing;

class Service {
  // @veridikt
  // kind: state
  // name: counter
  static int counter = 0;

  // @veridikt
  // purpose: "same-file call, root_relative import call, and a dropped call"
  void driver() {
    bump();
    Helper.run();
    Missing.gone();
    counter = counter + 1;
  }

  // @veridikt
  // purpose: "reads the state symbol — a non-write occurrence"
  int show() {
    return counter;
  }

  void bump() {}
}
