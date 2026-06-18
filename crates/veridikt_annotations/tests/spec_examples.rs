//! T2 exit criterion (G-13): every annotation block in the spec's own
//! examples scans and parses cleanly. Spec examples enter CI here.

use std::path::Path;

use veridikt_annotations::scan_source;
use veridikt_intent::parse_intent;

#[test]
fn every_spec_python_example_block_parses() {
    let spec_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/veridikt-spec.md");
    let spec = std::fs::read_to_string(spec_path).expect("spec must exist next to the crates");
    let mut total_blocks = 0;
    for (i, fence) in python_fences(&spec).iter().enumerate() {
        let name = format!("spec-example-{i}.py");
        let (blocks, findings) = scan_source(Path::new(&name), fence, "#");
        assert!(
            findings.is_empty(),
            "scan findings in fence {i}: {findings:?}"
        );
        for block in &blocks {
            let (_, parse_findings) = parse_intent(&block.raw_clauses);
            assert!(
                parse_findings.is_empty(),
                "parse findings in fence {i}, block at line {}: {parse_findings:?}",
                block.start_line
            );
            total_blocks += 1;
        }
    }
    // §7.1 has one block, §19 has three. If this drops, fence extraction broke.
    assert!(
        total_blocks >= 4,
        "expected at least 4 @veridikt blocks in spec python examples, found {total_blocks}"
    );
}

fn python_fences(md: &str) -> Vec<String> {
    let mut fences = Vec::new();
    let mut current: Option<String> = None;
    for line in md.lines() {
        match current.as_mut() {
            None => {
                if line.trim_start().starts_with("```python") {
                    current = Some(String::new());
                }
            }
            Some(buf) => {
                if line.trim_start().starts_with("```") {
                    fences.push(current.take().expect("checked Some"));
                } else {
                    buf.push_str(line);
                    buf.push('\n');
                }
            }
        }
    }
    fences
}
