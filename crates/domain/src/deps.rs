//! Prerequisites (SPEC §6.3b): "task B waits for task A". The graph must stay acyclic,
//! and a task is blocked while any prerequisite is still open.

use std::collections::{HashMap, HashSet};

/// Would giving `task` the prerequisites `new_deps` create a cycle? `deps_of` maps each
/// task to its current prerequisites (the entry for `task` itself is ignored).
pub fn creates_cycle(
    task: &str,
    new_deps: &[String],
    deps_of: &HashMap<String, Vec<String>>,
) -> bool {
    // A cycle exists if `task` is reachable from one of its new prerequisites.
    let mut stack: Vec<&str> = new_deps.iter().map(String::as_str).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    while let Some(t) = stack.pop() {
        if t == task {
            return true;
        }
        if !seen.insert(t) {
            continue;
        }
        if let Some(next) = deps_of.get(t) {
            stack.extend(next.iter().map(String::as_str));
        }
    }
    false
}

/// State of a prerequisite for blocking purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prereq {
    /// Still to do: blocks.
    Open,
    /// Done.
    Done,
    /// Skipped, won't do, missed or deleted: no longer blocks (D-6: auto-unblock).
    Dropped,
}

pub fn is_blocked(prereqs: &[Prereq]) -> bool {
    prereqs.contains(&Prereq::Open)
}

pub fn prereq_state(status: Option<&str>) -> Prereq {
    match status {
        Some("open") => Prereq::Open,
        Some("done") => Prereq::Done,
        _ => Prereq::Dropped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(edges: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
        edges
            .iter()
            .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
            .collect()
    }
    fn v(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cycles() {
        // fold -> dry -> wash
        let g = graph(&[("fold", &["dry"]), ("dry", &["wash"]), ("wash", &[])]);
        assert!(creates_cycle("wash", &v(&["fold"]), &g), "indirect");
        assert!(creates_cycle("wash", &v(&["wash"]), &g), "self");
        assert!(creates_cycle("dry", &v(&["fold"]), &g), "direct");
        assert!(!creates_cycle("fold", &v(&["wash", "dry"]), &g));
        assert!(!creates_cycle("iron", &v(&["dry"]), &g), "new task");
    }

    #[test]
    fn diamonds_are_fine() {
        // d -> b, c; b -> a; c -> a
        let g = graph(&[("b", &["a"]), ("c", &["a"]), ("d", &["b", "c"])]);
        assert!(!creates_cycle("e", &v(&["d", "a"]), &g));
        assert!(creates_cycle("a", &v(&["d"]), &g));
    }

    #[test]
    fn replacing_a_tasks_own_deps_is_not_a_cycle() {
        // Re-saving b with the same prerequisite must not look like a cycle via b's old entry.
        let g = graph(&[("b", &["a"])]);
        assert!(!creates_cycle("b", &v(&["a"]), &g));
    }

    #[test]
    fn blocking() {
        use Prereq::*;
        assert!(!is_blocked(&[]));
        assert!(is_blocked(&[Done, Open]));
        assert!(
            !is_blocked(&[Done, Dropped]),
            "skipped/deleted prerequisites unblock (D-6)"
        );
        assert_eq!(prereq_state(Some("open")), Open);
        assert_eq!(prereq_state(Some("wont_do")), Dropped);
        assert_eq!(prereq_state(None), Dropped, "deleted");
    }
}
