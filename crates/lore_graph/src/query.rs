//! Query parser (§10.1): query text in, `Query` out. A query that does not
//! parse is a usage error (exit 2, D-053a), not a §18 finding — the messages
//! still say what went wrong, where, and what to do (G-5).

use lore_intent::{Kind, QName};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub expr: QueryExpr,
    pub filters: Vec<Filter>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryExpr {
    Affects { arg: QName, star: bool },
    Reads { arg: QName, star: bool },
    Touches { arg: QName, star: bool },
    Triggers { arg: QName, star: bool },
    Emits { arg: QName },
    Handlers { arg: QName },
    Depends { arg: QName, star: bool },
    Dependents { arg: QName, star: bool },
    Reaches { arg: QName },
    Path { from: QName, to: QName },
    Show { arg: QName },
    Tagged(String),
    Owner(String),
    Unknown { scope: Option<(ScopeKind, QName)> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Module,
    Service,
    Workflow,
}

impl ScopeKind {
    pub fn name(self) -> &'static str {
        match self {
            ScopeKind::Module => "module",
            ScopeKind::Service => "service",
            ScopeKind::Workflow => "workflow",
        }
    }

    pub fn kind(self) -> Kind {
        match self {
            ScopeKind::Module => Kind::Module,
            ScopeKind::Service => Kind::Service,
            ScopeKind::Workflow => Kind::Workflow,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    InModule(QName),
    InService(QName),
    OwnedBy(String),
    Kind(Kind),
}

/// The §10.1 query forms, for the unknown-form suggestion.
const FORMS: [&str; 14] = [
    "affects",
    "reads",
    "touches",
    "triggers",
    "emits",
    "handlers",
    "depends",
    "dependents",
    "reaches",
    "path",
    "show",
    "tagged",
    "owner",
    "unknown",
];

/// Forms that accept the transitive `*` (§10.1).
const STARRED: [&str; 6] = [
    "affects",
    "reads",
    "touches",
    "triggers",
    "depends",
    "dependents",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Ident(String),
    Str(String),
    Star,
    LParen,
    RParen,
    Comma,
    Dot,
}

impl Tok {
    fn show(&self) -> String {
        match self {
            Tok::Ident(s) => format!("\"{s}\""),
            Tok::Str(_) => "a string".to_string(),
            Tok::Star => "\"*\"".to_string(),
            Tok::LParen => "\"(\"".to_string(),
            Tok::RParen => "\")\"".to_string(),
            Tok::Comma => "\",\"".to_string(),
            Tok::Dot => "\".\"".to_string(),
        }
    }
}

// @lore
// purpose: "Parse one §10.1 query string into the Query AST the engine executes"
// because: "Parse failures are usage errors with remedies in prose, not §18 findings: the query is ephemeral input, not project source"
pub fn parse(text: &str) -> Result<Query, String> {
    let toks = lex(text)?;
    let mut p = Parser { toks, pos: 0 };
    let expr = p.query_expr()?;
    let mut filters = Vec::new();
    while !p.at_end() {
        filters.push(p.filter()?);
    }
    Ok(Query { expr, filters })
}

fn lex(text: &str) -> Result<Vec<Tok>, String> {
    let mut toks = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\n' | '\r' => i += 1,
            '*' => {
                toks.push(Tok::Star);
                i += 1;
            }
            '(' => {
                toks.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                toks.push(Tok::RParen);
                i += 1;
            }
            ',' => {
                toks.push(Tok::Comma);
                i += 1;
            }
            '.' => {
                toks.push(Tok::Dot);
                i += 1;
            }
            '"' => {
                // §15 StringLit with D-045b escapes: \" and \\ unescape,
                // any other \ sequence is kept verbatim.
                let mut s = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None => {
                            return Err(format!(
                                "unterminated string in query \"{text}\"; close it with '\"'"
                            ));
                        }
                        Some('"') => {
                            i += 1;
                            break;
                        }
                        Some('\\') => match chars.get(i + 1) {
                            Some(e @ ('"' | '\\')) => {
                                s.push(*e);
                                i += 2;
                            }
                            _ => {
                                s.push('\\');
                                i += 1;
                            }
                        },
                        Some(c) => {
                            s.push(*c);
                            i += 1;
                        }
                    }
                }
                toks.push(Tok::Str(s));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                toks.push(Tok::Ident(chars[start..i].iter().collect()));
            }
            other => {
                return Err(format!(
                    "unexpected character '{other}' at column {} of query \"{text}\"",
                    i + 1
                ));
            }
        }
    }
    Ok(toks)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn at_end(&self) -> bool {
        self.pos >= self.toks.len()
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self, expected: &str) -> Result<Tok, String> {
        let t = self
            .toks
            .get(self.pos)
            .cloned()
            .ok_or_else(|| format!("query ends early; expected {expected}"))?;
        self.pos += 1;
        Ok(t)
    }

    fn expect(&mut self, want: Tok, expected: &str) -> Result<(), String> {
        let got = self.next(expected)?;
        if got == want {
            Ok(())
        } else {
            Err(format!("expected {expected}, found {}", got.show()))
        }
    }

    fn ident(&mut self, expected: &str) -> Result<String, String> {
        match self.next(expected)? {
            Tok::Ident(s) => Ok(s),
            other => Err(format!("expected {expected}, found {}", other.show())),
        }
    }

    fn string(&mut self, expected: &str) -> Result<String, String> {
        match self.next(expected)? {
            Tok::Str(s) => Ok(s),
            other => Err(format!("expected {expected}, found {}", other.show())),
        }
    }

    /// ref ::= Ident ("." Ident)*  (§10.1, dotted per D-051a)
    fn qname(&mut self, expected: &str) -> Result<QName, String> {
        let mut segments = vec![self.ident(expected)?];
        while self.peek() == Some(&Tok::Dot) {
            self.pos += 1;
            segments.push(self.ident("an identifier after \".\"")?);
        }
        Ok(QName(segments))
    }

    fn query_expr(&mut self) -> Result<QueryExpr, String> {
        let form = self.ident("a query form (affects, reads, touches, ...)")?;
        if !FORMS.contains(&form.as_str()) {
            let nearest = crate::util::nearest(&form, FORMS.iter().map(|s| s.to_string()))
                .expect("FORMS is non-empty");
            return Err(format!(
                "unknown query form \"{form}\"; did you mean \"{nearest}\"?"
            ));
        }
        let star = if self.peek() == Some(&Tok::Star) {
            self.pos += 1;
            if !STARRED.contains(&form.as_str()) {
                return Err(if form == "reaches" {
                    "\"reaches\" is always transitive; drop the \"*\"".to_string()
                } else {
                    format!(
                        "\"{form}\" does not take \"*\"; only {} do",
                        STARRED.join(", ")
                    )
                });
            }
            true
        } else {
            false
        };

        let expr = match form.as_str() {
            "affects" => QueryExpr::Affects {
                arg: self.one_ref(&form)?,
                star,
            },
            "reads" => QueryExpr::Reads {
                arg: self.one_ref(&form)?,
                star,
            },
            "touches" => QueryExpr::Touches {
                arg: self.one_ref(&form)?,
                star,
            },
            "triggers" => QueryExpr::Triggers {
                arg: self.one_ref(&form)?,
                star,
            },
            "emits" => QueryExpr::Emits {
                arg: self.one_ref(&form)?,
            },
            "handlers" => QueryExpr::Handlers {
                arg: self.one_ref(&form)?,
            },
            "depends" => QueryExpr::Depends {
                arg: self.one_ref(&form)?,
                star,
            },
            "dependents" => QueryExpr::Dependents {
                arg: self.one_ref(&form)?,
                star,
            },
            "reaches" => QueryExpr::Reaches {
                arg: self.one_ref(&form)?,
            },
            "show" => QueryExpr::Show {
                arg: self.one_ref(&form)?,
            },
            "path" => {
                self.expect(Tok::LParen, "\"(\" after \"path\"")?;
                let from = self.qname("a qname")?;
                self.expect(Tok::Comma, "\",\" between the two path endpoints")?;
                let to = self.qname("a qname")?;
                self.expect(Tok::RParen, "\")\"")?;
                QueryExpr::Path { from, to }
            }
            "tagged" => QueryExpr::Tagged(self.one_string(&form)?),
            "owner" => QueryExpr::Owner(self.one_string(&form)?),
            "unknown" => {
                let scope = if matches!(self.peek(), Some(Tok::Ident(k)) if k == "in") {
                    // `unknown in <kindword>(ref)` and a trailing `in` filter
                    // are distinguished by the kindword: workflow is scope-only.
                    let save = self.pos;
                    self.pos += 1;
                    let kindword = self.ident("\"module\", \"service\", or \"workflow\"")?;
                    let kind = match kindword.as_str() {
                        "module" => Some(ScopeKind::Module),
                        "service" => Some(ScopeKind::Service),
                        "workflow" => Some(ScopeKind::Workflow),
                        _ => None,
                    };
                    match kind {
                        Some(k) => {
                            self.expect(Tok::LParen, &format!("\"(\" after \"{kindword}\""))?;
                            let arg = self.qname("a qname")?;
                            self.expect(Tok::RParen, "\")\"")?;
                            Some((k, arg))
                        }
                        None => {
                            self.pos = save;
                            None
                        }
                    }
                } else {
                    None
                };
                QueryExpr::Unknown { scope }
            }
            _ => unreachable!("form membership checked above"),
        };
        Ok(expr)
    }

    fn one_ref(&mut self, form: &str) -> Result<QName, String> {
        self.expect(Tok::LParen, &format!("\"(\" after \"{form}\""))?;
        let arg = self.qname("a qname")?;
        self.expect(Tok::RParen, "\")\"")?;
        Ok(arg)
    }

    fn one_string(&mut self, form: &str) -> Result<String, String> {
        self.expect(Tok::LParen, &format!("\"(\" after \"{form}\""))?;
        let s = self.string("a quoted string")?;
        self.expect(Tok::RParen, "\")\"")?;
        Ok(s)
    }

    fn filter(&mut self) -> Result<Filter, String> {
        let word =
            self.ident("a filter (in module(...), in service(...), owned_by(...), kind(...))")?;
        match word.as_str() {
            "in" => {
                let kindword = self.ident("\"module\" or \"service\"")?;
                match kindword.as_str() {
                    "module" => Ok(Filter::InModule(self.one_ref("module")?)),
                    "service" => Ok(Filter::InService(self.one_ref("service")?)),
                    other => Err(format!(
                        "\"in\" filters take \"module\" or \"service\", not \"{other}\""
                    )),
                }
            }
            "owned_by" => Ok(Filter::OwnedBy(self.one_string("owned_by")?)),
            "kind" => {
                self.expect(Tok::LParen, "\"(\" after \"kind\"")?;
                let name = self.ident("a kind keyword (module, service, ..., external)")?;
                let kind = Kind::parse(&name).ok_or_else(|| {
                    format!(
                        "\"{name}\" is not a kind; use one of the §7.2 keywords (module, service, workflow, step, state, event, type, error, function, external)"
                    )
                })?;
                self.expect(Tok::RParen, "\")\"")?;
                Ok(Filter::Kind(kind))
            }
            other => Err(format!(
                "expected a filter (in module(...), in service(...), owned_by(\"...\"), kind(...)), found \"{other}\""
            )),
        }
    }
}
