//! Boundary tests (G-4): TypeScript source in -> exact derived node/edge
//! sets with confidences out, including required absences (G-7).
//! Unhappy paths first (G-11).

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

// ---- drops first (G-7, G-11) ----

#[test]
fn unresolvable_calls_are_dropped_and_counted() {
    // console.log: unknown object; fetch: builtin; this.run(): no local
    // construction; default imports drop (D-062c).
    let files = [
        unit("src/a.ts", "App", "export function f() {}\n"),
        unit(
            "src/m.ts",
            "App",
            "import f from \"./a\";\n\nclass C {\n  run() {\n    console.log(1);\n    fetch(\"u\");\n    this.run();\n    f();\n  }\n}\n",
        ),
    ];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(edges(&r), []);
    assert_eq!(r.unresolved_calls, 4);
}

#[test]
fn calls_inside_arrow_functions_and_at_module_level_drop() {
    let files = [unit(
        "src/m.ts",
        "App",
        "export function g() {}\n\ng();\nconst k = () => g();\n",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(edges(&r), []);
    assert_eq!(r.unresolved_calls, 2);
}

#[test]
fn non_relative_imports_drop() {
    let files = [unit(
        "src/m.ts",
        "App",
        "import { get } from \"axios\";\n\nfunction fetchIt() {\n  get(\"u\");\n}\n",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(edges(&r), []);
    assert_eq!(r.unresolved_calls, 1);
}

// ---- derived nodes ----

#[test]
fn declarations_become_nodes_variable_declarations_do_not() {
    let files = [unit(
        "src/m.ts",
        "App",
        "const ledger: number[] = [];\nexport function f() {}\nexport class C {\n  m() {}\n}\ninterface I {}\ntype T = string;\nenum E { A }\n",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(
        nodes(&r),
        [
            ("App.f".to_string(), "function"),
            ("App.C".to_string(), "type"),
            ("App.m".to_string(), "function"),
            ("App.I".to_string(), "type"),
            ("App.T".to_string(), "type"),
            ("App.E".to_string(), "type"),
        ]
    );
}

// ---- Calls edges ----

#[test]
fn same_file_calls_are_exact_and_local_method_calls_resolve() {
    let files = [unit(
        "src/m.ts",
        "App",
        "export class Store {\n  save() {}\n}\n\nfunction helper() {}\n\nexport function run() {\n  helper();\n  const s = new Store();\n  s.save();\n}\n",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(
        edges(&r),
        [
            e("App.run", "App.helper", "Calls", "Exact"),
            e("App.run", "App.save", "Calls", "Exact"),
        ]
    );
    assert_eq!(r.unresolved_calls, 0);
}

#[test]
fn relative_named_and_namespace_imports_resolve() {
    let files = [
        unit("src/pay/svc.ts", "Payment", "export function charge() {}\n"),
        unit(
            "src/user/u.ts",
            "User",
            "import { charge } from \"../pay/svc\";\nimport * as svc from \"../pay/svc\";\n\nexport function a() {\n  charge();\n}\nexport function b() {\n  svc.charge();\n}\n",
        ),
        unit(
            "src/user/idx.ts",
            "User",
            "import { entry } from \"./dir\";\n\nexport function c() {\n  entry();\n}\n",
        ),
        unit(
            "src/user/dir/index.ts",
            "User",
            "export function entry() {}\n",
        ),
    ];
    let r = derive(&cfg(), &common::packs(), &files, &[]);
    assert_eq!(
        edges(&r),
        [
            e("User.c", "User.entry", "Calls", "Resolved"),
            e("User.a", "Payment.charge", "Calls", "Resolved"),
            e("User.b", "Payment.charge", "Calls", "Resolved"),
        ]
    );
    assert_eq!(r.unresolved_calls, 0);
}

// ---- state touches ----

#[test]
fn mutator_calls_assignments_and_reads_classify_per_the_ts_table() {
    let files = [unit(
        "src/m.ts",
        "App",
        "const ledger: string[] = [];\nlet total = 0;\n\nexport function charge(x: string) {\n  ledger.push(x);\n  total += 1;\n}\n\nexport function report() {\n  return ledger.length + total;\n}\n",
    )];
    let states = [
        state("App.ledger", "ledger", "src/m.ts", "App"),
        state("App.total", "total", "src/m.ts", "App"),
    ];
    let r = derive(&cfg(), &common::packs(), &files, &states);
    assert_eq!(
        edges(&r),
        [
            e("App.charge", "App.ledger", "Affects", "Heuristic"),
            e("App.charge", "App.total", "Affects", "Heuristic"),
            e("App.report", "App.ledger", "Reads", "Heuristic"),
            e("App.report", "App.total", "Reads", "Heuristic"),
        ]
    );
}

#[test]
fn cross_file_touches_resolve_through_named_and_namespace_imports_only() {
    let files = [
        unit(
            "src/pay/svc.ts",
            "Payment",
            "export const ledger: number[] = [];\n",
        ),
        unit(
            "src/user/u.ts",
            "User",
            "import { ledger } from \"../pay/svc\";\nimport * as svc from \"../pay/svc\";\n\nexport function audit() {\n  svc.ledger.push(1);\n}\n",
        ),
        // same identifier, no import: not the same symbol
        unit(
            "src/other/o.ts",
            "Other",
            "export function peek() {\n  return ledger;\n}\n",
        ),
    ];
    let states = [state(
        "Payment.ledger",
        "ledger",
        "src/pay/svc.ts",
        "Payment",
    )];
    let r = derive(&cfg(), &common::packs(), &files, &states);
    assert_eq!(
        edges(&r),
        [e("User.audit", "Payment.ledger", "Affects", "Heuristic")]
    );
}
