import { run } from "./helpers";
import { ghost } from "./missing";

// @veridikt
// kind: state
// name: counter
let counter = 0;

// @veridikt
// purpose: "writes and reads the state symbol"
function bump() {
  counter = counter + 1;
}

// @veridikt
// purpose: "reads the state symbol — a non-write occurrence"
function show() {
  return counter;
}

// @veridikt
// purpose: "exact same-file call, resolved import call, and a dropped call"
function driver() {
  bump();
  run();
  ghost();
}
