import { run } from "./helpers";
import { ghost } from "./missing";

// @lore
// kind: state
// name: counter
let counter = 0;

// @lore
// purpose: "writes and reads the state symbol"
function bump() {
  counter = counter + 1;
}

// @lore
// purpose: "reads the state symbol — a non-write occurrence"
function show() {
  return counter;
}

// @lore
// purpose: "exact same-file call, resolved import call, and a dropped call"
function driver() {
  bump();
  run();
  ghost();
}
