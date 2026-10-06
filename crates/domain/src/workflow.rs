//! Workflow templates (SPEC §6.3b): which steps a run of a variant has, in order, and
//! how long each step waits after the previous one. v1 chains are linear.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub title: String,
    /// Wait this long after this step is done before the next step is ready.
    pub wait_min: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainStep<'a> {
    pub step: &'a Step,
    /// 1-based position within the chain (after skipping).
    pub number: u32,
    pub total: u32,
    /// Wait inherited from the previous included step (`None` for the first).
    pub wait_before_min: Option<u32>,
}

/// The chain for one run: the template's steps minus `skip`, each waiting for the one
/// before it. Waits of skipped steps are dropped (no machine runs for a skipped step).
pub fn chain<'a>(steps: &'a [Step], skip: &[String]) -> Vec<ChainStep<'a>> {
    let included: Vec<&Step> = steps.iter().filter(|s| !skip.contains(&s.id)).collect();
    let total = included.len() as u32;
    let mut prev_wait = None;
    included
        .into_iter()
        .enumerate()
        .map(|(i, step)| {
            let c = ChainStep {
                step,
                number: i as u32 + 1,
                total,
                wait_before_min: if i == 0 { None } else { prev_wait },
            };
            prev_wait = step.wait_min.filter(|m| *m > 0);
            c
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps() -> Vec<Step> {
        let s = |id: &str, wait: Option<u32>| Step {
            id: id.into(),
            title: id.into(),
            wait_min: wait,
        };
        vec![
            s("wash", Some(60)),
            s("dry", Some(45)),
            s("iron", None),
            s("fold", None),
        ]
    }

    #[test]
    fn full_chain_with_waits() {
        let st = steps();
        let c = chain(&st, &[]);
        assert_eq!(
            c.iter().map(|c| c.step.id.as_str()).collect::<Vec<_>>(),
            ["wash", "dry", "iron", "fold"]
        );
        assert_eq!(
            c.iter().map(|c| c.wait_before_min).collect::<Vec<_>>(),
            [None, Some(60), Some(45), None]
        );
        assert!(c.iter().all(|c| c.total == 4));
        assert_eq!(c[3].number, 4);
    }

    #[test]
    fn variants_skip_steps_and_relink() {
        let st = steps();
        // Delicates: no dryer, no ironing -> wash, then fold after the wash's wait.
        let c = chain(&st, &["dry".into(), "iron".into()]);
        assert_eq!(
            c.iter()
                .map(|c| (c.step.id.as_str(), c.number, c.total, c.wait_before_min))
                .collect::<Vec<_>>(),
            [("wash", 1, 2, None), ("fold", 2, 2, Some(60))]
        );
        // Towels: no ironing -> fold follows dry, after the dryer's 45 minutes.
        let c = chain(&st, &["iron".into()]);
        assert_eq!(c[2].step.id, "fold");
        assert_eq!(c[2].wait_before_min, Some(45));
    }

    #[test]
    fn skipping_everything_gives_nothing() {
        let st = steps();
        assert!(chain(&st, &st.iter().map(|s| s.id.clone()).collect::<Vec<_>>()).is_empty());
    }
}
