//! Printed day sheets (7.3): every printed task gets a short code, so a sheet and the
//! screen can be matched up at a glance. The code comes from the task's id, so the same
//! task carries the same code on every sheet.

/// Shortest code length; longer only when two tasks on one sheet would share a code.
const MIN_LEN: usize = 3;

/// Codes for the tasks on a sheet, in order: the last letters of each id, upper case,
/// as many as it takes to tell them apart.
pub fn codes(ids: &[&str]) -> Vec<String> {
    let longest = ids.iter().map(|id| id.len()).max().unwrap_or(0);
    let mut len = MIN_LEN.min(longest.max(1));
    loop {
        let out: Vec<String> = ids
            .iter()
            .map(|id| {
                let chars: Vec<char> = id.chars().collect();
                chars[chars.len().saturating_sub(len)..]
                    .iter()
                    .collect::<String>()
                    .to_uppercase()
            })
            .collect();
        let mut seen = std::collections::HashSet::new();
        if out.iter().all(|c| seen.insert(c.clone())) || len >= longest {
            return out;
        }
        len += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_stable_codes() {
        let a = "01JD0000000000000000000ABC";
        let b = "01JD0000000000000000000XYZ";
        assert_eq!(codes(&[a, b]), vec!["ABC", "XYZ"]);
        // The same task keeps its code on another sheet with other tasks.
        assert_eq!(codes(&[b]), vec!["XYZ"]);
        // Clashing endings get longer, for every task on that sheet.
        let c = "01JD0000000000000000001ABC";
        assert_eq!(codes(&[a, c, b]), vec!["0ABC", "1ABC", "0XYZ"]);
        assert!(codes(&[]).is_empty());
    }
}
