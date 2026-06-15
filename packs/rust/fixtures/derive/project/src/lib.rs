mod helpers;

use crate::helpers::run;
use crate::missing::ghost;

// @lore
// kind: state
// name: COUNTER
static mut COUNTER: u32 = 0;

// @lore
// purpose: "same-file call, resolved use-path call, dropped call, and a write"
fn driver() {
    local();
    run();
    ghost();
    unsafe {
        COUNTER += 1;
    }
}

// @lore
// purpose: "the same-file Exact-call target"
fn local() {}

// @lore
// purpose: "reads the state symbol — a non-write occurrence"
fn show() -> u32 {
    unsafe { COUNTER }
}
