//! Parent/child hierarchies (nested projects): cycle checks and subtree walks.

use std::collections::HashMap;

/// Whether making `new_parent` the parent of `node` would create a cycle, i.e.
/// whether `new_parent` is `node` itself or one of its descendants.
/// `parent_of` maps each id to its current parent.
pub fn would_cycle(
    node: &str,
    new_parent: &str,
    parent_of: &HashMap<String, Option<String>>,
) -> bool {
    let mut cur = Some(new_parent.to_string());
    let mut steps = 0;
    while let Some(id) = cur {
        if id == node {
            return true;
        }
        steps += 1;
        if steps > parent_of.len() + 1 {
            return true; // existing data already contains a cycle: refuse
        }
        cur = parent_of.get(&id).cloned().flatten();
    }
    false
}

/// `root` and all its descendants.
pub fn subtree(root: &str, parent_of: &HashMap<String, Option<String>>) -> Vec<String> {
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for (id, p) in parent_of {
        if let Some(p) = p {
            children.entry(p.as_str()).or_default().push(id.as_str());
        }
    }
    let mut out = vec![root.to_string()];
    let mut i = 0;
    while i < out.len() {
        if let Some(cs) = children.get(out[i].as_str()) {
            for c in cs {
                if !out.iter().any(|o| o == c) {
                    out.push(c.to_string());
                }
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(pairs: &[(&str, Option<&str>)]) -> HashMap<String, Option<String>> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.map(String::from)))
            .collect()
    }

    #[test]
    fn cycles() {
        // paper > writing > draft ; garden
        let t = tree(&[
            ("paper", None),
            ("writing", Some("paper")),
            ("draft", Some("writing")),
            ("garden", None),
        ]);
        assert!(would_cycle("paper", "paper", &t));
        assert!(would_cycle("paper", "draft", &t));
        assert!(would_cycle("writing", "draft", &t));
        assert!(!would_cycle("draft", "paper", &t));
        assert!(!would_cycle("garden", "draft", &t));
        assert!(!would_cycle("paper", "garden", &t));
    }

    #[test]
    fn corrupt_data_is_refused() {
        let t = tree(&[("a", Some("b")), ("b", Some("a")), ("c", None)]);
        assert!(would_cycle("c", "a", &t));
    }

    #[test]
    fn subtrees() {
        let t = tree(&[
            ("paper", None),
            ("research", Some("paper")),
            ("writing", Some("paper")),
            ("draft", Some("writing")),
            ("x", None),
        ]);
        let mut s = subtree("paper", &t);
        s.sort();
        assert_eq!(s, ["draft", "paper", "research", "writing"]);
        assert_eq!(subtree("x", &t), ["x"]);
    }
}
