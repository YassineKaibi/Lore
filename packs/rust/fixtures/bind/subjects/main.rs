// @lore
// purpose: "a function subject (@subject.function)"
fn alpha() {}

// @lore
// purpose: "a struct subject (@subject.type)"
struct Beta;

// @lore
// purpose: "an enum subject (@subject.type)"
enum Gamma {
    A,
}

// @lore
// kind: state
// name: DELTA
const DELTA: u32 = 0;

// @lore
// purpose: "attributed struct — exercises the attribute_item sibling skip (D-050c)"
#[derive(Debug)]
struct Epsilon;
