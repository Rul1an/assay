pub mod judge_cache;
pub mod rows;
pub mod schema;
pub mod store;

pub use store::Store;

pub(crate) fn now_rfc3339ish() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::now_rfc3339ish;

    #[test]
    fn now_rfc3339ish_is_utc_rfc3339_millis() -> anyhow::Result<()> {
        let ts = now_rfc3339ish();
        let parsed = chrono::DateTime::parse_from_rfc3339(&ts)?;

        assert_eq!(parsed.offset().local_minus_utc(), 0);
        assert!(ts.ends_with('Z'));

        let frac = ts
            .split('.')
            .nth(1)
            .and_then(|rest| rest.strip_suffix('Z'))
            .expect("timestamp must carry millisecond precision");
        assert_eq!(frac.len(), 3);
        assert!(frac.chars().all(|c| c.is_ascii_digit()));

        Ok(())
    }
}
