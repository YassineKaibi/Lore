mod helpers;

use crate::helpers::run;
use crate::missing::ghost;

// @veridikt
// kind: state
// name: COUNTER
static mut COUNTER: u32 = 0;

// @veridikt
// purpose: "same-file call, resolved use-path call, dropped call, and a write"
fn driver() {
    local();
    run();
    ghost();
    unsafe {
        COUNTER += 1;
    }
}

// @veridikt
// purpose: "the same-file Exact-call target"
fn local() {}

// @veridikt
// purpose: "reads the state symbol — a non-write occurrence"
fn show() -> u32 {
    unsafe { COUNTER }
}
