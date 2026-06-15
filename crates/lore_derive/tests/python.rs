//! Boundary tests (G-4): Python source in -> exact derived node/edge sets
//! with confidences out, including required absences — calls that MUST be
//! dropped, never guessed (G-7). Unhappy paths first (G-11).

use lore_derive::{DeriveConfig, DeriveResult, SourceUnit, StateSymbol, derive};
use lore_intent::QName;

mod common;

fn cfg() -> DeriveConfig {
    DeriveConfig {
        roots: vec!["src".into()],
        cache_dir: None,
        manifests: Vec::new(),
    }
}

fn unit(path: &str, module: &str, text: &str) -> SourceUnit {
    SourceUnit {
        path: path.into(),
        text: text.to_string(),
        module: module.to_string(),
    }
}

fn state(qname: &str, identifier: &str, file: &str, module: &str) -> StateSymbol {
    StateSymbol {
        qname: QName::from_dotted(qname),
        identifier: identifier.to_string(),
        file: file.into(),
        module: module.to_string(),
    }
}

/// (from, to, kind, confidence) — the assertion surface.
fn edges(r: &DeriveResult) -> Vec<(String, String, String, String)> {
    r.edges
        .iter()
        .map(|e| {
            (
                e.from.to_string(),
                e.to.to_string(),
                format!("{:?}", e.kind),
                format!("{:?}", e.confidence),
            )
        })
        .collect()
}

fn nodes(r: &DeriveResult) -> Vec<(String, &'static str)> {
    r.nodes
        .iter()
        .map(|n| (n.qname.to_string(), n.kind.name()))
        .collect()
}

fn e(from: &str, to: &str, kind: &str, conf: &str) -> (String, String, String, String) {
    (from.into(), to.into(), kind.into(), conf.into())
}

// ---- drops first (G-7, G-11): required absences are the contract ----

#[test]
fn unresolvable_calls_are_dropped_and_counted_never_guessed() {
    let files = [unit(
        "src/m.py",
        "App",
        "def f():\n    print(\"x\")\n    helper.run()\n    a.b.c()\n",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(edges(&r), []);
    assert_eq!(r.unresolved_calls, 3);
}

#[test]
fn calls_without_an_honest_attribution_drop() {
    // module level, class level, and inside a lambda: no enclosing derived
    // function, so the edge would be a guess (D-062a).
    let files = [unit(
        "src/m.py",
        "App",
        "def g():\n    pass\n\ng()\n\nclass C:\n    x = g()\n\nh = lambda: g()\n",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(edges(&r), []);
    assert_eq!(r.unresolved_calls, 3);
}

#[test]
fn colliding_derived_qnames_are_excluded_and_counted_not_guessed() {
    // Two `__init__`s share the qname App.__init__ (D-060d): no node, no
    // finding, counted — an error here would fail every unannotated repo.
    let files = [unit(
        "src/m.py",
        "App",
        "class A:\n    def __init__(self):\n        pass\n\nclass B:\n    def __init__(self):\n        pass\n",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(
        nodes(&r),
        [("App.A".to_string(), "type"), ("App.B".to_string(), "type")]
    );
    assert_eq!(r.ambiguous_names, 2);
}

#[test]
fn constructor_calls_resolve_to_a_type_and_drop() {
    let files = [unit(
        "src/m.py",
        "App",
        "class Foo:\n    pass\n\ndef make():\n    return Foo()\n",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(edges(&r), []);
    assert_eq!(r.unresolved_calls, 1);
}

#[test]
fn imports_that_leave_derivation_scope_drop() {
    // `requests` resolves to no in-scope file: rule 3, dropped and counted.
    let files = [unit(
        "src/m.py",
        "App",
        "from requests import get\n\ndef fetch():\n    get(\"u\")\n",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(edges(&r), []);
    assert_eq!(r.unresolved_calls, 1);
}

#[test]
fn method_calls_without_a_local_construction_drop() {
    // `s` is a parameter: its type is not syntactically evident (§8.2).
    let files = [unit(
        "src/m.py",
        "App",
        "class Store:\n    def save(self):\n        pass\n\ndef other(s):\n    s.save()\n",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(edges(&r), []);
    assert_eq!(r.unresolved_calls, 1);
}

// ---- derived nodes ----

#[test]
fn declarations_become_function_and_type_nodes_assignments_do_not() {
    let files = [unit(
        "src/m.py",
        "App",
        "ledger = []\n\ndef f():\n    pass\n\nclass C:\n    def m(self):\n        pass\n",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(
        nodes(&r),
        [
            ("App.f".to_string(), "function"),
            ("App.C".to_string(), "type"),
            ("App.m".to_string(), "function"),
        ]
    );
    assert_eq!(r.scope, [std::path::PathBuf::from("src/m.py")]);
    let f = &r.nodes[0];
    assert_eq!(f.loc.line, 3); // the def line, where the binder also points
    assert_eq!(f.origin, lore_intent::Origin::Derived);
}

// ---- Calls edges ----

#[test]
fn same_file_calls_are_exact_including_recursion() {
    let files = [unit(
        "src/m.py",
        "App",
        "def helper():\n    pass\n\ndef main():\n    helper()\n    main()\n",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(
        edges(&r),
        [
            e("App.main", "App.helper", "Calls", "Exact"),
            e("App.main", "App.main", "Calls", "Exact"),
        ]
    );
    assert_eq!(r.unresolved_calls, 0);
}

#[test]
fn nested_functions_derive_and_attribute_their_own_calls() {
    let files = [unit(
        "src/m.py",
        "App",
        "def outer():\n    def inner():\n        outer()\n    inner()\n",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(
        edges(&r),
        [
            e("App.inner", "App.outer", "Calls", "Exact"),
            e("App.outer", "App.inner", "Calls", "Exact"),
        ]
    );
}

#[test]
fn from_import_calls_resolve_across_files() {
    let files = [
        unit("src/pay/svc.py", "Payment", "def charge():\n    pass\n"),
        unit(
            "src/user/u.py",
            "User",
            "from pay.svc import charge\n\ndef signup():\n    charge()\n",
        ),
    ];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(
        edges(&r),
        [e("User.signup", "Payment.charge", "Calls", "Resolved")]
    );
    assert_eq!(r.unresolved_calls, 0);
}

#[test]
fn aliased_imports_resolve_one_level_deep_only() {
    let files = [
        unit("src/pay/svc.py", "Payment", "def charge():\n    pass\n"),
        unit(
            "src/user/u.py",
            "User",
            "import pay.svc as svc\nimport pay.svc\nfrom pay.svc import charge as pay_up\n\ndef a():\n    svc.charge()\n\ndef b():\n    pay_up()\n\ndef c():\n    pay.svc.charge()\n",
        ),
    ];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(
        edges(&r),
        [
            e("User.a", "Payment.charge", "Calls", "Resolved"),
            e("User.b", "Payment.charge", "Calls", "Resolved"),
        ]
    );
    // `pay.svc.charge()` is a dotted callee deeper than alias.name: dropped.
    assert_eq!(r.unresolved_calls, 1);
}

#[test]
fn method_call_on_a_locally_constructed_instance_is_exact() {
    let files = [unit(
        "src/m.py",
        "App",
        "class Store:\n    def save(self):\n        pass\n\ndef run():\n    s = Store()\n    s.save()\n",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(edges(&r), [e("App.run", "App.save", "Calls", "Exact")]);
    // the Store() construction itself resolves to a Type: dropped, counted
    assert_eq!(r.unresolved_calls, 1);
}

// ---- state touches (§8.3) ----

#[test]
fn touches_classify_writes_and_reads_and_dedupe_per_function() {
    let files = [unit(
        "src/m.py",
        "App",
        "ledger = []\nbalances = {}\n\ndef charge(user, amount):\n    if balances.get(user, 0) < amount:\n        return\n    ledger.append(entry(user))\n    ledger.append(entry(user))\n    total = balances\n",
    )];
    let states = [
        state("App.ledger", "ledger", "src/m.py", "App"),
        state("App.balances", "balances", "src/m.py", "App"),
    ];
    let r = derive(&cfg(), &common::packs(), &files, &states);
    assert_eq!(
        edges(&r),
        [
            e("App.charge", "App.balances", "Reads", "Heuristic"),
            e("App.charge", "App.ledger", "Affects", "Heuristic"),
        ]
    );
    // the dedupe keeps the first occurrence's span (D-062d)
    assert_eq!(r.edges[0].loc.line, 5);
    assert_eq!(r.edges[1].loc.line, 7);
}

#[test]
fn assignment_augmented_assignment_and_mutators_are_writes_the_rest_reads() {
    let files = [unit(
        "src/m.py",
        "App",
        "count = 0\nitems = []\n\ndef reset():\n    count = 0\n\ndef bump():\n    count += 1\n\ndef peek():\n    return items.copy()\n",
    )];
    let states = [
        state("App.count", "count", "src/m.py", "App"),
        state("App.items", "items", "src/m.py", "App"),
    ];
    let r = derive(&cfg(), &common::packs(), &files, &states);
    assert_eq!(
        edges(&r),
        [
            e("App.reset", "App.count", "Affects", "Heuristic"),
            e("App.bump", "App.count", "Affects", "Heuristic"),
            e("App.peek", "App.items", "Reads", "Heuristic"),
        ]
    );
}

#[test]
fn module_level_touches_produce_no_edges() {
    // the definition itself and a module-level mutation: no enclosing
    // function, nothing to attribute (D-062d)
    let files = [unit("src/m.py", "App", "ledger = []\nledger.append(1)\n")];
    let states = [state("App.ledger", "ledger", "src/m.py", "App")];
    let r = derive(&cfg(), &common::packs(), &files, &states);
    assert_eq!(edges(&r), []);
}

#[test]
fn cross_module_touches_need_an_import_that_resolves_to_the_defining_file() {
    let files = [
        unit("src/pay/svc.py", "Payment", "ledger = []\n"),
        unit(
            "src/user/u.py",
            "User",
            "from pay.svc import ledger\n\ndef audit():\n    ledger.append(1)\n",
        ),
        // same identifier, no import: silence — not the same symbol
        unit(
            "src/other/o.py",
            "Other",
            "def peek():\n    return ledger\n",
        ),
        // an import of the same name from a file that is NOT the defining
        // file: silence — the symbol is somebody else's
        unit("src/user/fake.py", "User", "ledger = []\n"),
        unit(
            "src/user/u2.py",
            "User",
            "from user.fake import ledger\n\ndef shadow():\n    ledger.append(2)\n",
        ),
    ];
    let states = [state(
        "Payment.ledger",
        "ledger",
        "src/pay/svc.py",
        "Payment",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &states);
    assert_eq!(
        edges(&r),
        [e("User.audit", "Payment.ledger", "Affects", "Heuristic")]
    );
}

#[test]
fn whole_import_alias_attribute_touches_resolve() {
    let files = [
        unit("src/pay/svc.py", "Payment", "ledger = []\n"),
        unit(
            "src/user/u.py",
            "User",
            "import pay.svc as svc\n\ndef audit():\n    svc.ledger.append(1)\n    n = svc.ledger\n",
        ),
    ];
    let states = [state(
        "Payment.ledger",
        "ledger",
        "src/pay/svc.py",
        "Payment",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &states);
    assert_eq!(
        edges(&r),
        [
            e("User.audit", "Payment.ledger", "Affects", "Heuristic"),
            e("User.audit", "Payment.ledger", "Reads", "Heuristic"),
        ]
    );
}

#[test]
fn import_statements_themselves_are_not_occurrences() {
    let files = [
        unit("src/pay/svc.py", "Payment", "ledger = []\n"),
        unit("src/user/u.py", "User", "from pay.svc import ledger\n"),
    ];
    let states = [state(
        "Payment.ledger",
        "ledger",
        "src/pay/svc.py",
        "Payment",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &states);
    assert_eq!(edges(&r), []);
}
