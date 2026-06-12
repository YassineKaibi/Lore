//! Node table construction: duplicate qnames (E0305, D-016), ambient Module
//! nodes from the manifest's `[modules]` mapping (D-046), and the
//! declared/derived merge (§8.1, D-060).

use std::collections::{HashMap, HashSet};

use lore_intent::{Finding, Intent, IntentNode, Kind, Origin, QName, Spanned};

use crate::{Ctx, OwnedFinding};

pub(crate) fn build(
    declared: Vec<IntentNode>,
    manifest_modules: &[Spanned<String>],
    derived: Vec<IntentNode>,
    findings: &mut Vec<OwnedFinding>,
) -> Ctx {
    let mut nodes: HashMap<QName, IntentNode> = HashMap::new();
    let mut order: Vec<QName> = Vec::new();
    let mut annotated: HashSet<QName> = HashSet::new();

    for node in declared {
        if let Some(first) = nodes.get(&node.qname) {
            findings.push(OwnedFinding::new(
                Finding::new(
                    "E0305",
                    node.loc.clone(),
                    format!(
                        "duplicate qname \"{}\"; already declared at {}:{}; rename one or scope them into different modules",
                        node.qname,
                        first.loc.file.display(),
                        first.loc.line
                    ),
                ),
                &node.qname,
            ));
            continue; // first declaration wins; never merge (D-016)
        }
        order.push(node.qname.clone());
        annotated.insert(node.qname.clone());
        nodes.insert(node.qname.clone(), node);
    }

    // A lore.toml mapping originates a Module node (§4) unless a scoping
    // block already declared it. Ambient nodes carry no intent block, so
    // requirement checks skip them (D-046).
    for m in manifest_modules {
        let qname = QName::from_dotted(&m.value);
        if nodes.contains_key(&qname) {
            continue;
        }
        order.push(qname.clone());
        nodes.insert(
            qname.clone(),
            IntentNode {
                qname,
                kind: Kind::Module,
                origin: Origin::Declared,
                intent: Intent::default(),
                loc: m.span.clone(),
            },
        );
    }

    // The derived layer (§8.1): a derived node naming the same declaration
    // as a declared node (same file, same start line) merges into it with
    // origin Both — the declared kind, intent, and loc win (D-060b). The
    // same qname on a *different* declaration is E0305: the declared node
    // wins, the derived node is rejected (D-060c). Derived-only collisions
    // never reach this table — lore_derive excludes them (D-060d).
    let mut rejected_derived: HashSet<QName> = HashSet::new();
    for node in derived {
        match nodes.get_mut(&node.qname) {
            Some(existing) => {
                let same_declaration =
                    existing.loc.file == node.loc.file && existing.loc.line == node.loc.line;
                if same_declaration {
                    existing.origin = Origin::Both;
                } else {
                    findings.push(OwnedFinding::new(
                        Finding::new(
                            "E0305",
                            node.loc.clone(),
                            format!(
                                "duplicate qname \"{}\": this declaration collides with the one declared at {}:{}; give the annotated one a distinct \"name:\"",
                                node.qname,
                                existing.loc.file.display(),
                                existing.loc.line
                            ),
                        ),
                        &node.qname,
                    ));
                    rejected_derived.insert(node.qname.clone());
                }
            }
            None => {
                order.push(node.qname.clone());
                nodes.insert(node.qname.clone(), node);
            }
        }
    }

    Ctx {
        nodes,
        order,
        annotated,
        rejected_derived,
    }
}
