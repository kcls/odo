use chrono::DateTime;
use chrono_tz::Tz;

/// Format an RFC3339 date string optionally translating the result into the
/// provided timezone.
///
/// If no timezone parameter is provided the date remains in the source timezone.
///
/// Fails if the date or timezone string is unparseable.
///
/// Fails if the source string is not fully qualified with a timezone,
/// i.e. no assumptions are made about what the time zone of the source
/// date should be.
///
/// # Example
/// ```
/// use odo_client::date;
///
/// let date_str = date::format_datetime_str(
///     "2025-10-10T14:24:40+02:00",
///     "%m/%d/%y %H:%M",
///     Some("Australia/Perth") // no DST
/// ).unwrap();
///
/// assert_eq!(date_str, "10/10/25 20:24");
///
/// // Naive dates (no time / timezone) will fail.
/// assert!(date::format_datetime_str("2025-10-10", "%F", None).is_err());
///
/// // Naive datetimes (no timezone) will fail.
/// assert!(date::format_datetime_str("2025-10-10T12:00:00", "%F", None).is_err());
/// ```
pub fn format_datetime_str(
    date_str: &str,
    format: &str,
    timezone: Option<&str>,
) -> Result<String, String> {
    let dt = DateTime::parse_from_rfc3339(date_str)
        .map_err(|e| format!("Cannot parse date '{date_str}': {e}"))?;

    if let Some(tz_str) = timezone {
        let tz: Tz = tz_str
            .parse()
            .map_err(|e| format!("Invalid timezone '{tz_str}': {e}"))?;
        Ok(dt.with_timezone(&tz).format(format).to_string())
    } else {
        Ok(dt.format(format).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_valid_data() {
        let result = format_datetime_str(
            "2025-10-10T14:24:40+02:00",
            "%m/%d/%y %H:%M",
            Some("Australia/Perth"),
        )
        .unwrap();

        assert_eq!(result, "10/10/25 20:24");
    }

    #[test]
    fn handles_invalid_timezone() {
        let result = format_datetime_str(
            "2025-01-15T20:30:00+00:00",
            "%m/%d/%y %I:%M %p",
            Some("Invalid/Timezone"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn handles_invalid_datetime_string() {
        let result = format_datetime_str("not-a-date", "%m/%d/%y %I:%M %p", None);
        assert!(result.is_err());
    }
}
