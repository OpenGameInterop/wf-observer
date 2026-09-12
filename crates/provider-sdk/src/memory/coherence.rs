/// Reads twice and returns the second value when both observations agree.
///
/// The caller bounds each read. Agreement does not prove atomicity or exclude
/// values changing and changing back between reads.
///
/// # Errors
///
/// Propagates a read error immediately, or calls `changed` when the values differ.
pub fn read_stable<T, E>(
    mut read: impl FnMut() -> Result<T, E>,
    changed: impl FnOnce() -> E,
) -> Result<T, E>
where
    T: Eq,
{
    let first = read()?;
    let second = read()?;
    if first == second {
        Ok(second)
    } else {
        Err(changed())
    }
}

#[cfg(test)]
mod tests {
    use super::read_stable;

    #[test]
    fn stable_reads_require_agreement_and_stop_on_read_error() {
        for (values, expected, remaining) in [
            ([Ok(7), Ok(7)], Ok(7), 0),
            ([Ok(7), Ok(8)], Err("changed"), 0),
            ([Err("read failed"), Ok(7)], Err("read failed"), 1),
        ] {
            let mut values = values.into_iter();
            assert_eq!(
                read_stable(|| values.next().unwrap_or(Err("exhausted")), || "changed"),
                expected
            );
            assert_eq!(values.len(), remaining);
        }
    }
}
