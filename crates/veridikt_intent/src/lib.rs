//! Shared intent-model contracts (spec §13). These are the AST contract (G-3):
//! changing a field here is a breaking change for every downstream crate.

mod pack;
mod parse;

pub use pack::{ImportStrategy, PackSpec, Tier, WholeAlias};
pub use parse::parse_intent;

use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub file: PathBuf,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QName(pub Vec<String>);

impl QName {
    pub fn from_dotted(s: &str) -> Self {
        QName(s.split('.').map(str::to_owned).collect())
    }
}

impl fmt::Display for QName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.join("."))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Module,
    Service,
    Workflow,
    Step,
    State,
    Event,
    Type,
    Error,
    Function,
    External,
}

impl Kind {
    /// The ten lowercase keywords of §7.2.
    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "module" => Kind::Module,
            "service" => Kind::Service,
            "workflow" => Kind::Workflow,
            "step" => Kind::Step,
            "state" => Kind::State,
            "event" => Kind::Event,
            "type" => Kind::Type,
            "error" => Kind::Error,
            "function" => Kind::Function,
            "external" => Kind::External,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Module => "module",
            Kind::Service => "service",
            Kind::Workflow => "workflow",
            Kind::Step => "step",
            Kind::State => "state",
            Kind::Event => "event",
            Kind::Type => "type",
            Kind::Error => "error",
            Kind::Function => "function",
            Kind::External => "external",
        }
    }

    /// Capitalized form used in JSON output (§10.4 prints "Function").
    pub fn display(self) -> &'static str {
        match self {
            Kind::Module => "Module",
            Kind::Service => "Service",
            Kind::Workflow => "Workflow",
            Kind::Step => "Step",
            Kind::State => "State",
            Kind::Event => "Event",
            Kind::Type => "Type",
            Kind::Error => "Error",
            Kind::Function => "Function",
            Kind::External => "External",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Declared,
    Derived,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    pub segments: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enforcement {
    Strict,
    Warn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub method: Option<HttpMethod>,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Intent {
    pub purpose: Option<Spanned<String>>,
    pub owner: Option<Spanned<String>>,
    pub because: Vec<Spanned<String>>,
    pub unknown: Vec<Spanned<String>>,
    pub assumes: Vec<Spanned<String>>,
    pub affects: Vec<Spanned<Ref>>,
    pub reads: Vec<Spanned<Ref>>,
    pub triggers: Vec<Spanned<Ref>>,
    pub emits: Vec<Spanned<Ref>>,
    pub on: Vec<Spanned<Ref>>,
    pub depends_on: Vec<Spanned<Ref>>,
    pub route: Option<Spanned<Route>>,
    pub enforcement: Option<Spanned<Enforcement>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentNode {
    pub qname: QName,
    pub kind: Kind,
    pub origin: Origin,
    pub intent: Intent,
    pub loc: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

// @veridikt
// kind: type
// purpose: "One diagnostic: a §18 registry code, severity derived from the code letter (D-040), a span, and a remedy-bearing message"
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub code: &'static str,
    pub severity: Severity,
    pub span: Span,
    pub message: String,
}

impl Finding {
    /// Severity is derived from the code letter (D-040). Panics on a code
    /// outside the §18 registry shape -- that is a programmer error.
    pub fn new(code: &'static str, span: Span, message: String) -> Self {
        let severity = match code.as_bytes()[0] {
            b'E' => Severity::Error,
            b'W' => Severity::Warning,
            _ => panic!("diagnostic code outside the §18 registry: {code}"),
        };
        Finding {
            code,
            severity,
            span,
            message,
        }
    }
}
