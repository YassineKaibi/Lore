// @lore
// purpose: "a function subject (@subject.function)"
function alpha() {}

// @lore
// purpose: "a class subject (@subject.type)"
class Beta {}

// @lore
// purpose: "an interface subject (@subject.type)"
interface Gamma {
  x: number;
}

// @lore
// kind: state
// name: delta
let delta = 0;

// @lore
// purpose: "exported function — exercises the export_statement wrapper descent"
export function epsilon() {}
