//! Staleness metadata gathering (§9.2, D-068): one `git blame
//! --line-porcelain` per file holding annotation blocks, committer-time per
//! line. The CLI owns the git boundary; `lore_graph::build` applies the
//! strictly-later comparison and emits W0301.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use lore_annotations::ScannedBlock;
use lore_graph::StalenessRecord;
use lore_intent::Span;

/// One blamed source line: commit hash, committer-time, committer-tz.
struct LineBlame {
    hash: String,
    time: i64,
    tz: String,
}

/// Gather a StalenessRecord per block with a subject span. None when not in
/// a git work tree (with one stderr notice, suppressed by --quiet) — the
/// graph then skips the check entirely (D-068c). Files whose blame fails
/// (untracked) are skipped silently: no history, nothing to be stale
/// against (D-068d).

// @lore
// name: gather_staleness
// purpose: "Blame every annotated file once and reduce each block to the two timestamps §9.2 compares"
// because: "Staleness is the one reconciliation input only git knows; the CLI gathers it so the graph stays a pure function of its inputs (D-068)"
pub fn gather(root: &Path, blocks: &[ScannedBlock], quiet: bool) -> Option<Vec<StalenessRecord>> {
    let inside = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output();
    let inside = inside
        .is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "true");
    if !inside {
        if !quiet {
            eprintln!("note: not a git work tree; staleness check (W0301) skipped");
        }
        return None;
    }

    let mut by_file: BTreeMap<&Path, Vec<&ScannedBlock>> = BTreeMap::new();
    for b in blocks {
        if b.subject_span.is_some() {
            by_file.entry(b.file.as_path()).or_default().push(b);
        }
    }

    let mut records = Vec::new();
    for (file, blocks) in by_file {
        let Some(lines) = blame(root, file) else {
            continue;
        };
        for b in blocks {
            let (s_start, s_end) = b.subject_span.expect("filtered to subject blocks");
            let Some((t_block, t_block_iso, _)) = newest(&lines, b.block_span.0, b.block_span.1)
            else {
                continue;
            };
            let Some((t_subject, t_subject_iso, subject_commit)) = newest(&lines, s_start, s_end)
            else {
                continue;
            };
            records.push(StalenessRecord {
                qname: b.qname.clone(),
                span: Span {
                    file: b.file.clone(),
                    line: b.block_span.0,
                    col: 1,
                    end_line: b.block_span.1,
                    end_col: 1,
                },
                t_block,
                t_subject,
                t_block_iso,
                t_subject_iso,
                subject_commit,
            });
        }
    }
    Some(records)
}

/// Blame one file into per-line (hash, committer-time, tz), indexed by
/// final line number. None on any git failure.
fn blame(root: &Path, file: &Path) -> Option<Vec<Option<LineBlame>>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["blame", "--line-porcelain", "--"])
        .arg(file)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);

    let mut lines: Vec<Option<LineBlame>> = Vec::new();
    let (mut hash, mut time, mut tz) = (String::new(), 0i64, "+0000".to_string());
    let mut lineno = 0usize;
    for l in text.lines() {
        if l.starts_with('\t') {
            // the content line closes one --line-porcelain group
            if lineno > 0 {
                if lines.len() < lineno {
                    lines.resize_with(lineno, || None);
                }
                lines[lineno - 1] = Some(LineBlame {
                    hash: hash.clone(),
                    time,
                    tz: tz.clone(),
                });
            }
        } else if let Some(rest) = l.strip_prefix("committer-time ") {
            time = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = l.strip_prefix("committer-tz ") {
            tz = rest.trim().to_string();
        } else {
            // group header: <40-hex hash> <orig line> <final line> [<count>]
            let mut parts = l.split(' ');
            if let (Some(h), Some(_), Some(fl)) = (parts.next(), parts.next(), parts.next())
                && h.len() == 40
                && h.bytes().all(|b| b.is_ascii_hexdigit())
                && let Ok(fl) = fl.parse::<usize>()
            {
                hash = h.to_string();
                lineno = fl;
            }
        }
    }
    Some(lines)
}

/// Max committer-time over a 1-based inclusive line range; the first line
/// attaining the max supplies the hash (D-068d). None when no line in the
/// range was blamed (span past EOF).
fn newest(lines: &[Option<LineBlame>], start: u32, end: u32) -> Option<(i64, String, String)> {
    let mut best: Option<&LineBlame> = None;
    for n in start..=end {
        let Some(Some(lb)) = lines.get(n as usize - 1) else {
            continue;
        };
        if best.is_none_or(|b| lb.time > b.time) {
            best = Some(lb);
        }
    }
    best.map(|lb| (lb.time, iso_strict(lb.time, &lb.tz), lb.hash.clone()))
}

/// Render epoch seconds + a blame tz offset ("+0130") as ISO-strict local
/// time, matching `git log --date=iso-strict` (D-059 surface parity).
fn iso_strict(epoch: i64, tz: &str) -> String {
    let offset = parse_tz(tz);
    let local = epoch + offset;
    let days = local.div_euclid(86_400);
    let secs = local.rem_euclid(86_400);
    let (y, mo, d) = civil_from_days(days);
    let (sign, off) = if offset < 0 {
        ('-', -offset)
    } else {
        ('+', offset)
    };
    format!(
        "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}{sign}{:02}:{:02}",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60,
        off / 3600,
        off % 3600 / 60,
    )
}

/// "+HHMM" / "-HHMM" -> seconds; anything else is UTC.
fn parse_tz(tz: &str) -> i64 {
    let (sign, digits) = match tz.split_at_checked(1) {
        Some(("+", d)) => (1, d),
        Some(("-", d)) => (-1, d),
        _ => return 0,
    };
    let Ok(hhmm) = digits.parse::<i64>() else {
        return 0;
    };
    sign * (hhmm / 100 * 3600 + hhmm % 100 * 60)
}

/// Days since 1970-01-01 -> (year, month, day). Howard Hinnant's
/// civil_from_days, exact over the whole i64 range we can meet.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
