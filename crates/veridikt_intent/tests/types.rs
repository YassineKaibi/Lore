use std::path::PathBuf;
use veridikt_intent::*;

fn span() -> Span {
    Span {
        file: PathBuf::from("a.py"),
        line: 1,
        col: 1,
        end_line: 1,
        end_col: 5,
    }
}

#[test]
fn qname_displays_dotted() {
    assert_eq!(
        QName::from_dotted("Payment.ledger").to_string(),
        "Payment.ledger"
    );
    assert_eq!(
        QName::from_dotted("Payment.ledger").0,
        vec!["Payment", "ledger"]
    );
}

#[test]
fn kind_parses_all_ten_lowercase_keywords_and_rejects_junk() {
    for (s, k) in [
        ("module", Kind::Module),
        ("service", Kind::Service),
        ("workflow", Kind::Workflow),
        ("step", Kind::Step),
        ("state", Kind::State),
        ("event", Kind::Event),
        ("type", Kind::Type),
        ("error", Kind::Error),
        ("function", Kind::Function),
        ("external", Kind::External),
    ] {
        assert_eq!(Kind::parse(s), Some(k));
        assert_eq!(k.name(), s);
    }
    assert_eq!(Kind::parse("Module"), None);
    assert_eq!(Kind::parse("klass"), None);
}

#[test]
fn finding_severity_comes_from_code_letter() {
    assert_eq!(
        Finding::new("E0102", span(), "m".into()).severity,
        Severity::Error
    );
    assert_eq!(
        Finding::new("W0208", span(), "m".into()).severity,
        Severity::Warning
    );
}

#[test]
fn intent_default_is_empty() {
    let i = Intent::default();
    assert!(i.purpose.is_none() && i.because.is_empty() && i.affects.is_empty());
}
