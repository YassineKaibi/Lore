//! Shared "nearest existing name" machinery: E0306 messages (§6.3) and the
//! D-053a unresolved-query-argument message both suggest by edit distance.

/// Nearest candidate by edit distance; ties go to the lexically smaller
/// string so messages are deterministic. None iff `candidates` is empty.
pub(crate) fn nearest<I: IntoIterator<Item = String>>(
    missing: &str,
    candidates: I,
) -> Option<String> {
    candidates
        .into_iter()
        .min_by_key(|c| (levenshtein(missing, c), c.clone()))
}

pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b_chars.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut row = Vec::with_capacity(b_chars.len() + 1);
        row.push(i + 1);
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            row.push((prev[j] + cost).min(prev[j + 1] + 1).min(row[j] + 1));
        }
        prev = row;
    }
    *prev.last().expect("row is non-empty")
}
