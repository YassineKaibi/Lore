// @veridikt
// purpose: "a function subject (@subject.function)"
function alpha() {}

// @veridikt
// purpose: "a class subject (@subject.type)"
class Beta {}

// @veridikt
// purpose: "an interface subject (@subject.type)"
interface Gamma {
  x: number;
}

// @veridikt
// kind: state
// name: delta
let delta = 0;

// @veridikt
// purpose: "exported function — exercises the export_statement wrapper descent"
export function epsilon() {}
