//! The daily health check-in (D-75): one answer per day, with what kind of sickness or
//! where the injury is. Only recorded: it changes nothing else.

pub const STATUSES: &[&str] = &["great", "ok", "unwell", "sick", "injured"];
pub const SICK_KINDS: &[&str] = &["cold", "flu", "fever", "stomach", "headache", "other"];
pub const INJURY_KINDS: &[&str] = &["back", "neck", "hand", "arm", "knee", "foot", "other"];
pub const MAX_NOTE: usize = 200;

/// Check a status and its optional kind. Only sick and injured days take a kind, and it
/// must be one of theirs.
pub fn validate(status: &str, kind: Option<&str>) -> Result<(), &'static str> {
    if !STATUSES.contains(&status) {
        return Err("status must be great, ok, unwell, sick or injured");
    }
    let Some(kind) = kind else { return Ok(()) };
    match status {
        "sick" if SICK_KINDS.contains(&kind) => Ok(()),
        "injured" if INJURY_KINDS.contains(&kind) => Ok(()),
        "sick" | "injured" => Err("unknown kind for that status"),
        _ => Err("only sick or injured days have a kind"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_and_kinds() {
        assert!(validate("great", None).is_ok());
        assert!(validate("sick", None).is_ok());
        assert!(validate("sick", Some("flu")).is_ok());
        assert!(validate("injured", Some("back")).is_ok());
        assert!(validate("injured", Some("other")).is_ok());
        assert!(validate("sick", Some("back")).is_err());
        assert!(validate("injured", Some("flu")).is_err());
        assert!(validate("ok", Some("flu")).is_err());
        assert!(validate("tired", None).is_err());
    }
}
