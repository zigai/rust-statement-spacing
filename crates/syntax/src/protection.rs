use ra_ap_syntax::{SyntaxKind, SyntaxNode};
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct Protection {
    known: HashMap<SyntaxNode, bool>,
}

impl Protection {
    pub(crate) fn protected(&mut self, node: &SyntaxNode) -> bool {
        let mut pending = Vec::new();
        let mut inherited = false;
        for ancestor in node.ancestors() {
            if let Some(&known) = self.known.get(&ancestor) {
                inherited = known;
                break;
            }
            pending.push(ancestor);
        }
        for ancestor in pending.into_iter().rev() {
            inherited = inherited || attributes_protect(&ancestor);
            self.known.insert(ancestor, inherited);
        }
        return inherited;
    }
}

fn attributes_protect(ancestor: &SyntaxNode) -> bool {
    let derived = ancestor.kind() != SyntaxKind::SOURCE_FILE
        && ancestor
            .children()
            .filter(|child| {
                return child.kind() == SyntaxKind::ATTR;
            })
            .any(|attr| {
                return attr
                    .text()
                    .to_string()
                    .chars()
                    .filter(|c| {
                        return !c.is_whitespace();
                    })
                    .collect::<String>()
                    .starts_with("#[derive(");
            });
    return ancestor
        .children()
        .filter(|child| return child.kind() == SyntaxKind::ATTR)
        .any(|attr| {
            let compact: String = attr
                .text()
                .to_string()
                .chars()
                .filter(|c| return !c.is_whitespace())
                .collect();

            // A conditional skip is conservatively respected, rather than
            // attempting to duplicate rustfmt's cfg evaluation.
            if compact.contains("rustfmt::skip") {
                return true;
            }

            let name = compact
                .trim_start_matches("#![")
                .trim_start_matches("#[")
                .split(['(', '=', ']'])
                .next()
                .unwrap_or("");

            // Unknown procedural attributes may rewrite original syntax. Do not
            // assume a valid original-source mapping through such a transform.
            let built_in = matches!(
                name,
                "allow"
                    | "warn"
                    | "deny"
                    | "forbid"
                    | "expect"
                    | "cfg"
                    | "cfg_attr"
                    | "derive"
                    | "doc"
                    | "inline"
                    | "cold"
                    | "must_use"
                    | "test"
                    | "ignore"
                    | "should_panic"
                    | "track_caller"
                    | "repr"
                    | "deprecated"
                    | "non_exhaustive"
                    | "no_mangle"
                    | "export_name"
                    | "link_name"
                    | "link"
                    | "path"
                    | "macro_export"
                    | "macro_use"
                    | "automatically_derived"
                    | "unsafe"
                    | "target_feature"
                    | "no_std"
                    | "no_main"
                    | "recursion_limit"
                    | "type_length_limit"
                    | "feature"
            );
            let derive_helper = derived
                && matches!(
                    name,
                    "serde" | "clap" | "command" | "arg" | "value" | "schemars" | "strum"
                );
            return !built_in && !derive_helper;
        });
}

pub(crate) fn inside_token_tree(node: &SyntaxNode) -> bool {
    return node.ancestors().any(|ancestor| {
        return matches!(
            ancestor.kind(),
            SyntaxKind::TOKEN_TREE | SyntaxKind::MACRO_RULES
        );
    });
}
