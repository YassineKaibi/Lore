//! Clause parser (spec §5, §7.2, the §15 clause productions; D-045).
//! Input is the scanner's reassembled clause lines; output is the §13
//! `Intent`. Diagnostics: E0202 unknown clause, E0206 duplicate singular
//! clause, E0207 malformed clause.

use std::path::Path;

use crate::{Enforcement, Finding, HttpMethod, Intent, Ref, Route, Span, Spanned};

/// The §3.1 clause set, in canonical order. Suggestion ties (E0202) go to
/// the earlier name.
const CLAUSES: [&str; 13] = [
    "purpose",
    "owner",
    "because",
    "unknown",
    "assumes",
    "affects",
    "reads",
    "triggers",
    "emits",
    "on",
    "depends_on",
    "route",
    "enforcement",
];

const ROUTE_MSG: &str = "\"route\" expects METHOD \"<path>\" or \"<path>\", where METHOD is GET, POST, PUT, DELETE, or PATCH";

/// Parse a block's clause lines into its `Intent`. Each input is one logical
/// clause as reassembled by the scanner (§7.2): newlines occur only inside
/// quoted strings. A clause that fails to parse contributes nothing (D-045);
/// whitespace-only lines are visual separators and are skipped.

// @lore
// purpose: "Parse a block's reassembled clause lines into the shared Intent AST, with per-value spans"
// because: "A clause that fails to parse contributes nothing to Intent -- a half-parsed claim is a guess (D-045, G-7)"
pub fn parse_intent(clauses: &[Spanned<String>]) -> (Intent, Vec<Finding>) {
    let mut intent = Intent::default();
    let mut findings = Vec::new();
    for clause in clauses {
        if clause.value.trim().is_empty() {
            continue;
        }
        if let Err(f) = parse_clause(clause, &mut intent) {
            findings.push(f);
        }
    }
    (intent, findings)
}

fn parse_clause(line: &Spanned<String>, intent: &mut Intent) -> Result<(), Finding> {
    let mut cur = Cursor::new(&line.value, &line.span);
    cur.eat_ws();
    let name_start = cur.mark();
    let Some(name) = cur.ident() else {
        return Err(Finding::new(
            "E0207",
            cur.span_to_end(),
            "malformed clause; a clause line is \"<name>: <value>\"".to_string(),
        ));
    };
    let name_span = cur.span_from(name_start);
    cur.eat_ws();
    if !cur.eat(':') {
        return Err(Finding::new(
            "E0207",
            name_span,
            format!("malformed clause; expected \":\" after \"{name}\""),
        ));
    }
    if !CLAUSES.contains(&name.as_str()) {
        let nearest = nearest_clause(&name);
        return Err(Finding::new(
            "E0202",
            name_span,
            format!("unknown clause \"{name}\"; did you mean \"{nearest}\"?"),
        ));
    }
    cur.eat_ws();
    match name.as_str() {
        "purpose" => {
            if let Some(first) = &intent.purpose {
                return Err(duplicate(&name, name_span, first.span.line));
            }
            let value = parse_string(&mut cur, &name)?;
            finish(&mut cur, &name)?;
            intent.purpose = Some(value);
        }
        "owner" => {
            if let Some(first) = &intent.owner {
                return Err(duplicate(&name, name_span, first.span.line));
            }
            let value = parse_string(&mut cur, &name)?;
            finish(&mut cur, &name)?;
            intent.owner = Some(value);
        }
        "because" | "unknown" | "assumes" => {
            let value = parse_string(&mut cur, &name)?;
            finish(&mut cur, &name)?;
            match name.as_str() {
                "because" => intent.because.push(value),
                "unknown" => intent.unknown.push(value),
                _ => intent.assumes.push(value),
            }
        }
        "affects" | "reads" | "triggers" | "emits" | "on" | "depends_on" => {
            let refs = parse_ref_list(&mut cur, &name)?;
            finish(&mut cur, &name)?;
            match name.as_str() {
                "affects" => &mut intent.affects,
                "reads" => &mut intent.reads,
                "triggers" => &mut intent.triggers,
                "emits" => &mut intent.emits,
                "on" => &mut intent.on,
                _ => &mut intent.depends_on,
            }
            .extend(refs);
        }
        "route" => {
            if let Some(first) = &intent.route {
                return Err(duplicate(&name, name_span, first.span.line));
            }
            let start = cur.mark();
            let method = if cur.peek() == Some('"') {
                None
            } else {
                let word = cur.ident().unwrap_or_default();
                let method = match word.as_str() {
                    "GET" => HttpMethod::Get,
                    "POST" => HttpMethod::Post,
                    "PUT" => HttpMethod::Put,
                    "DELETE" => HttpMethod::Delete,
                    "PATCH" => HttpMethod::Patch,
                    _ => {
                        return Err(Finding::new(
                            "E0207",
                            cur.span_from(start),
                            ROUTE_MSG.to_string(),
                        ));
                    }
                };
                cur.eat_ws();
                if cur.peek() != Some('"') {
                    return Err(Finding::new(
                        "E0207",
                        cur.span_from(start),
                        ROUTE_MSG.to_string(),
                    ));
                }
                Some(method)
            };
            let path = parse_string(&mut cur, &name)?.value;
            let span = cur.span_from(start);
            finish(&mut cur, &name)?;
            intent.route = Some(Spanned {
                value: Route { method, path },
                span,
            });
        }
        "enforcement" => {
            if let Some(first) = &intent.enforcement {
                return Err(duplicate(&name, name_span, first.span.line));
            }
            let start = cur.mark();
            let word = cur.ident().unwrap_or_default();
            let value = match word.as_str() {
                "strict" => Enforcement::Strict,
                "warn" => Enforcement::Warn,
                _ => {
                    return Err(Finding::new(
                        "E0207",
                        cur.span_to_end(),
                        format!("\"enforcement\" must be \"strict\" or \"warn\", got \"{word}\""),
                    ));
                }
            };
            let span = cur.span_from(start);
            finish(&mut cur, &name)?;
            intent.enforcement = Some(Spanned { value, span });
        }
        _ => unreachable!("name was checked against CLAUSES"),
    }
    Ok(())
}

fn duplicate(name: &str, span: Span, first_line: u32) -> Finding {
    Finding::new(
        "E0206",
        span,
        format!(
            "duplicate \"{name}\" clause; \"{name}\" appears at most once per block (first at line {first_line}); remove one"
        ),
    )
}

/// `StringLit` per §15, with D-045 escapes: `\"` and `\\` are unescaped, any
/// other `\` sequence is kept verbatim. Newlines inside the literal are
/// preserved (multi-line strings, §7.2). Span covers the quotes.
fn parse_string(cur: &mut Cursor, clause: &str) -> Result<Spanned<String>, Finding> {
    if cur.peek() != Some('"') {
        return Err(Finding::new(
            "E0207",
            cur.span_to_end(),
            format!("\"{clause}\" expects a quoted string, e.g. {clause}: \"...\""),
        ));
    }
    let start = cur.mark();
    cur.bump();
    let mut value = String::new();
    loop {
        match cur.bump() {
            Some('"') => break,
            Some('\\') => match cur.bump() {
                Some('"') => value.push('"'),
                Some('\\') => value.push('\\'),
                Some(other) => {
                    value.push('\\');
                    value.push(other);
                }
                None => return Err(unterminated(cur, start, clause)),
            },
            Some(c) => value.push(c),
            None => return Err(unterminated(cur, start, clause)),
        }
    }
    Ok(Spanned {
        value,
        span: cur.span_from(start),
    })
}

fn unterminated(cur: &Cursor, start: (u32, u32), clause: &str) -> Finding {
    Finding::new(
        "E0207",
        cur.span_from(start),
        format!("unterminated string in \"{clause}\" clause; add a closing '\"'"),
    )
}

fn parse_ref_list(cur: &mut Cursor, clause: &str) -> Result<Vec<Spanned<Ref>>, Finding> {
    let mut refs = vec![parse_ref(cur, clause)?];
    loop {
        cur.eat_ws();
        if !cur.eat(',') {
            break;
        }
        cur.eat_ws();
        refs.push(parse_ref(cur, clause)?);
    }
    Ok(refs)
}

/// `ref ::= Ident ("." Ident)*` (§5.1). Each ref carries its own span.
fn parse_ref(cur: &mut Cursor, clause: &str) -> Result<Spanned<Ref>, Finding> {
    let start = cur.mark();
    let mut segments = Vec::new();
    loop {
        match cur.ident() {
            Some(seg) => segments.push(seg),
            None => {
                return Err(Finding::new(
                    "E0207",
                    cur.span_to_end(),
                    format!(
                        "\"{clause}\" expects one or more dotted refs like Payment.ledger, separated by commas"
                    ),
                ));
            }
        }
        if !cur.eat('.') {
            break;
        }
    }
    Ok(Spanned {
        value: Ref { segments },
        span: cur.span_from(start),
    })
}

/// After the value: only trailing whitespace is legal.
fn finish(cur: &mut Cursor, clause: &str) -> Result<(), Finding> {
    cur.eat_ws();
    if cur.at_end() {
        return Ok(());
    }
    let start = cur.mark();
    let rest = cur.rest().trim_end().to_string();
    while cur.bump().is_some() {}
    Err(Finding::new(
        "E0207",
        cur.span_from(start),
        format!("unexpected text after \"{clause}\" clause value: \"{rest}\""),
    ))
}

fn nearest_clause(got: &str) -> &'static str {
    CLAUSES
        .iter()
        .copied()
        .min_by_key(|c| levenshtein(got, c))
        .expect("CLAUSES is non-empty")
}

fn levenshtein(a: &str, b: &str) -> usize {
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

/// Character cursor over one logical clause line, tracking the source
/// position (1-based line, 1-based byte column) from the clause's base span.
/// A `\n` (only legal inside a string) resets the column to 1, matching the
/// scanner's per-line comment stripping.
struct Cursor<'a> {
    text: &'a str,
    file: &'a Path,
    pos: usize,
    line: u32,
    col: u32,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str, base: &'a Span) -> Self {
        Cursor {
            text,
            file: &base.file,
            pos: 0,
            line: base.line,
            col: base.col,
        }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += c.len_utf8() as u32;
        }
        Some(c)
    }

    fn eat(&mut self, want: char) -> bool {
        if self.peek() == Some(want) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.bump();
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.text.len()
    }

    fn rest(&self) -> &'a str {
        &self.text[self.pos..]
    }

    fn ident(&mut self) -> Option<String> {
        match self.peek() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
            _ => return None,
        }
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == '_') {
            self.bump();
        }
        Some(self.text[start..self.pos].to_string())
    }

    fn mark(&self) -> (u32, u32) {
        (self.line, self.col)
    }

    fn span_from(&self, (line, col): (u32, u32)) -> Span {
        Span {
            file: self.file.to_path_buf(),
            line,
            col,
            end_line: self.line,
            end_col: self.col,
        }
    }

    fn span_to_end(&self) -> Span {
        let (mut line, mut col) = (self.line, self.col);
        for c in self.rest().chars() {
            if c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += c.len_utf8() as u32;
            }
        }
        Span {
            file: self.file.to_path_buf(),
            line: self.line,
            col: self.col,
            end_line: line,
            end_col: col,
        }
    }
}
