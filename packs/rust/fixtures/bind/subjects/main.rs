// @veridikt
// purpose: "a function subject (@subject.function)"
fn alpha() {}

// @veridikt
// purpose: "a struct subject (@subject.type)"
struct Beta;

// @veridikt
// purpose: "an enum subject (@subject.type)"
enum Gamma {
    A,
}

// @veridikt
// kind: state
// name: DELTA
const DELTA: u32 = 0;

// @veridikt
// purpose: "attributed struct — exercises the attribute_item sibling skip (D-050c)"
#[derive(Debug)]
struct Epsilon;

struct World {
    // @veridikt
    // kind: state
    // purpose: "a struct field subject (@subject.value, D-084)"
    cells: u32,
}
