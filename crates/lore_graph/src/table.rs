//! Node table construction: duplicate qnames (E0305, D-016) and ambient
//! Module nodes from the manifest's `[modules]` mapping (D-046).

use std::collections::{HashMap, HashSet};

use lore_intent::{Finding, Intent, IntentNode, Kind, Origin, QName, Spanned};

use crate::{Ctx, OwnedFinding};

pub(crate) fn build(
    declared: Vec<IntentNode>,
    manifest_modules: &[Spanned<String>],
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

    Ctx {
        nodes,
        order,
        annotated,
    }
}
