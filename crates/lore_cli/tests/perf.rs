//! T6 exit criterion: the full pipeline (scan + derive + graph + lint) over
//! a ~20k-LOC Python tree completes < 10 s cold and < 1 s with a warm
//! extraction cache (§10.7, D-064). The roadmap names an OSS repo for the
//! manual run; CI pins the budget on a synthetic tree of the same size.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

const MODULES: usize = 10;
const FILES_PER_MODULE: usize = 10;
const FNS_PER_FILE: usize = 20;

fn generate(root: &Path) {
    let mut manifest = String::from(
        "[project]\nname = \"perf\"\nlanguages = [\"python\"]\nroots = [\"src\"]\n\n[modules]\n",
    );
    for m in 0..MODULES {
        writeln!(manifest, "\"src/m{m}/**\" = \"M{m}\"").unwrap();
    }
    std::fs::write(root.join("lore.toml"), manifest).unwrap();

    for m in 0..MODULES {
        let dir = root.join(format!("src/m{m}"));
        std::fs::create_dir_all(&dir).unwrap();
        for f in 0..FILES_PER_MODULE {
            let mut src = String::new();
            // one annotated state per module, in its first file
            if f == 0 {
                src.push_str(
                    "# @lore\n# kind: state\n# purpose: \"Module counter\"\ncounter = []\n\n",
                );
            }
            // a cross-module import that resolves inside the scope
            writeln!(
                src,
                "from m{}.f0 import fn0_1 as ext_call\n",
                (m + 1) % MODULES
            )
            .unwrap();
            for k in 0..FNS_PER_FILE {
                writeln!(src, "def fn{f}_{k}(x):").unwrap();
                src.push_str("    a = x + 1\n    b = a * 2\n");
                src.push_str("    if b > 3:\n        b = b - 1\n");
                src.push_str("    c = [a, b]\n    total = len(c)\n");
                if k > 0 {
                    writeln!(src, "    fn{f}_{}(total)", k - 1).unwrap();
                }
                src.push_str("    ext_call(total)\n");
                src.push_str("    log(total)\n"); // unresolvable: dropped, counted
                src.push_str("    counter.append(total)\n"); // heuristic state touch
                src.push_str("    return total\n\n");
            }
            std::fs::write(dir.join(format!("f{f}.py")), src).unwrap();
        }
    }
}

fn lint(root: &Path) -> Duration {
    let started = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_lore"))
        .args(["lint", "--no-color", "--quiet"])
        .current_dir(root)
        .output()
        .unwrap();
    let elapsed = started.elapsed();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    elapsed
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "the T6 10s/1s budget is for the shipped (release) build; CI runs this test with --release"
)]
fn full_pipeline_on_a_20k_loc_python_tree_meets_the_t6_budget() {
    let tmp = tempfile::tempdir().unwrap();
    generate(tmp.path());

    let cold = lint(tmp.path());
    assert!(
        cold < Duration::from_secs(10),
        "cold run took {cold:?}, over the 10s budget"
    );

    let warm = lint(tmp.path());
    assert!(
        warm < Duration::from_secs(1),
        "warm run took {warm:?}, over the 1s budget"
    );
}
