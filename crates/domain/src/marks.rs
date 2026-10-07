//! Project marks: every project gets a colour and a shape, so it can be told apart at a
//! glance in lists (a red square, a blue circle…). Mirrored in `web/src/lib/marks.ts`.

/// Colours in the order they're handed out (gray is kept for ideas).
pub const PALETTE: [&str; 9] = [
    "#dc2626", "#1e66cc", "#16a34a", "#ea580c", "#7c3aed", "#0891b2", "#db2777", "#ca8a04",
    "#4f46e5",
];
pub const SHAPES: [&str; 6] = ["circle", "square", "triangle", "diamond", "hexagon", "star"];

/// All 54 colour/shape pairs in the order they're handed out: each round uses every
/// colour once, and shapes shift by one per round so no pair repeats.
fn sequence() -> impl Iterator<Item = (&'static str, &'static str)> {
    (0..SHAPES.len()).flat_map(|round| {
        PALETTE
            .iter()
            .enumerate()
            .map(move |(c, color)| (*color, SHAPES[(c + round) % SHAPES.len()]))
    })
}

/// Mark for a new top-level project: the first pair nobody uses yet, or, once all are
/// taken, the least-used one (earliest in the sequence on ties).
pub fn pick_top(used: &[(&str, &str)]) -> (&'static str, &'static str) {
    let count = |pair: (&str, &str)| used.iter().filter(|u| **u == pair).count();
    sequence()
        .enumerate()
        .min_by_key(|(i, pair)| (count(*pair), *i))
        .map(|(_, pair)| pair)
        .unwrap_or((PALETTE[0], SHAPES[0]))
}

/// Shape for a new subproject (it keeps its parent's colour): the first shape that
/// neither the parent nor a sibling has, or the least-used one, avoiding the parent's.
pub fn pick_sub(parent: &str, siblings: &[&str]) -> &'static str {
    let count = |s: &str| siblings.iter().filter(|x| **x == s).count() + usize::from(parent == s);
    SHAPES
        .iter()
        .enumerate()
        .min_by_key(|(i, s)| (count(s), **s == parent, *i))
        .map(|(_, s)| *s)
        .unwrap_or(SHAPES[0])
}

pub fn is_shape(s: &str) -> bool {
    SHAPES.contains(&s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hands_out_every_pair_once() {
        let all: Vec<_> = sequence().collect();
        assert_eq!(all.len(), 54);
        let mut dedup = all.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(dedup.len(), 54, "no pair repeats");
        // The first round mixes shapes rather than nine circles.
        assert_eq!(
            &all[..3],
            &[
                ("#dc2626", "circle"),
                ("#1e66cc", "square"),
                ("#16a34a", "triangle")
            ]
        );
    }

    #[test]
    fn top_level_projects_get_unused_pairs_then_least_used() {
        assert_eq!(pick_top(&[]), ("#dc2626", "circle"));
        assert_eq!(pick_top(&[("#dc2626", "circle")]), ("#1e66cc", "square"));
        // A gap left by a deleted project is reused first.
        assert_eq!(pick_top(&[("#1e66cc", "square")]), ("#dc2626", "circle"));
        let everything: Vec<_> = sequence().collect();
        assert_eq!(pick_top(&everything), ("#dc2626", "circle"));
        let mut more = everything.clone();
        more.push(("#dc2626", "circle"));
        assert_eq!(pick_top(&more), ("#1e66cc", "square"));
    }

    #[test]
    fn subprojects_get_a_shape_their_parent_and_siblings_dont_have() {
        assert_eq!(pick_sub("circle", &[]), "square");
        assert_eq!(pick_sub("square", &["circle"]), "triangle");
        assert_eq!(
            pick_sub(
                "circle",
                &["square", "triangle", "diamond", "hexagon", "star"]
            ),
            "square"
        );
        assert!(is_shape("hexagon") && !is_shape("blob"));
    }
}
