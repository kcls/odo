//! JSON log output shared by the odo-* services.
//!
//! The formatter mirrors tracing-subscriber's built-in JSON layout
//! (timestamp, level, flattened event fields, filename, line_number,
//! current span) and adds process-level Kubernetes identity fields so
//! every line, including startup and background-worker output, can be
//! attributed to a pod once it reaches a JSON-aware log receiver.

use std::fmt;

use chrono::{SecondsFormat, Utc};
use serde::ser::{SerializeMap, Serializer};
use serde_json::Value;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::format::{JsonFields, Writer};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields, FormattedFields};
use tracing_subscriber::registry::LookupSpan;

/// Environment variables populated by the deployments' downward API
/// blocks, and the JSON key each one is emitted under.
const K8S_ENV_FIELDS: &[(&str, &str)] = &[
    ("pod", "K8S_POD_NAME"),
    ("namespace", "K8S_NAMESPACE"),
    ("node", "K8S_NODE_NAME"),
    ("service", "K8S_SERVICE"),
];

pub fn init(default_filter: &str) {
    tracing_subscriber::fmt()
        .fmt_fields(JsonFields::new())
        .event_format(K8sJson::from_env())
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.parse().unwrap()),
        )
        .init();
}

/// Event formatter producing one JSON object per line with a fixed set
/// of extra top-level fields.
pub struct K8sJson {
    static_fields: Vec<(&'static str, String)>,
}

impl K8sJson {
    /// Read the Kubernetes identity fields from the environment. Unset or
    /// empty variables are omitted, so local runs produce the same layout
    /// minus those keys.
    pub fn from_env() -> Self {
        let static_fields = K8S_ENV_FIELDS
            .iter()
            .filter_map(|(key, var)| {
                std::env::var(var)
                    .ok()
                    .filter(|v| !v.is_empty())
                    .map(|v| (*key, v))
            })
            .collect();
        Self { static_fields }
    }

    pub fn with_fields(static_fields: Vec<(&'static str, String)>) -> Self {
        Self { static_fields }
    }
}

impl<S, N> FormatEvent<S, N> for K8sJson
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();

        let mut fields = FieldCollector::default();
        event.record(&mut fields);

        let mut buf = Vec::with_capacity(256);
        let mut ser = serde_json::Serializer::new(&mut buf);
        let mut map = ser.serialize_map(None).map_err(|_| fmt::Error)?;

        let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true);
        map.serialize_entry("timestamp", &timestamp)
            .map_err(|_| fmt::Error)?;
        map.serialize_entry("level", &meta.level().to_string())
            .map_err(|_| fmt::Error)?;

        for (name, value) in &fields.0 {
            map.serialize_entry(name, value).map_err(|_| fmt::Error)?;
        }

        if let Some(file) = meta.file() {
            map.serialize_entry("filename", file)
                .map_err(|_| fmt::Error)?;
        }
        if let Some(line) = meta.line() {
            map.serialize_entry("line_number", &line)
                .map_err(|_| fmt::Error)?;
        }

        for (name, value) in &self.static_fields {
            map.serialize_entry(name, value).map_err(|_| fmt::Error)?;
        }

        let current = event
            .parent()
            .and_then(|id| ctx.span(id))
            .or_else(|| ctx.lookup_current());
        if let Some(span) = current {
            let ext = span.extensions();
            let raw = ext
                .get::<FormattedFields<N>>()
                .map(|f| f.fields.as_str())
                .unwrap_or("");
            map.serialize_entry(
                "span",
                &SpanFields {
                    raw,
                    name: span.name(),
                },
            )
            .map_err(|_| fmt::Error)?;
        }

        map.end().map_err(|_| fmt::Error)?;

        let line = std::str::from_utf8(&buf).map_err(|_| fmt::Error)?;
        writer.write_str(line)?;
        writer.write_char('\n')
    }
}

/// The current span's fields plus its name, in the same shape the
/// built-in JSON formatter produces.
struct SpanFields<'a> {
    raw: &'a str,
    name: &'a str,
}

impl serde::Serialize for SpanFields<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        // JsonFields stores span fields as a serialized JSON object.
        match serde_json::from_str::<Value>(self.raw) {
            Ok(Value::Object(obj)) => {
                for (k, v) in &obj {
                    map.serialize_entry(k, v)?;
                }
            }
            _ if self.raw.is_empty() => {}
            _ => map.serialize_entry("fields", self.raw)?,
        }
        map.serialize_entry("name", self.name)?;
        map.end()
    }
}

#[derive(Default)]
struct FieldCollector(Vec<(&'static str, Value)>);

impl FieldCollector {
    fn push(&mut self, field: &Field, value: Value) {
        self.0.push((field.name(), value));
    }
}

impl Visit for FieldCollector {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.push(field, Value::from(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.push(field, Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.push(field, Value::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.push(field, Value::from(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.push(field, Value::from(value));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.push(field, Value::from(value.to_string()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.push(field, Value::from(format!("{value:?}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Capture {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn capture_lines(format: K8sJson, f: impl FnOnce()) -> Vec<Value> {
        let capture = Capture::default();
        let sink = capture.clone();
        let subscriber = tracing_subscriber::fmt()
            .fmt_fields(JsonFields::new())
            .event_format(format)
            .with_writer(move || sink.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, f);

        let bytes = capture.0.lock().unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        text.lines()
            .map(|l| serde_json::from_str(l).expect("each line is valid JSON"))
            .collect()
    }

    fn k8s_format() -> K8sJson {
        K8sJson::with_fields(vec![
            ("pod", "odo-notify-7c9d".into()),
            ("namespace", "odo-pub".into()),
            ("node", "worker-1".into()),
            ("service", "odo-notify".into()),
        ])
    }

    #[test]
    fn emits_static_fields_and_flattened_event_fields() {
        let lines = capture_lines(k8s_format(), || {
            tracing::info!(status = 200u16, elapsed_ms = 6u64, "API Request completed");
        });

        assert_eq!(lines.len(), 1);
        let line = &lines[0];
        assert_eq!(line["level"], "INFO");
        assert_eq!(line["message"], "API Request completed");
        assert_eq!(line["status"], 200);
        assert_eq!(line["elapsed_ms"], 6);
        assert_eq!(line["pod"], "odo-notify-7c9d");
        assert_eq!(line["namespace"], "odo-pub");
        assert_eq!(line["node"], "worker-1");
        assert_eq!(line["service"], "odo-notify");
        assert!(line["filename"].as_str().unwrap().ends_with("logging.rs"));
        assert!(line["line_number"].is_u64());
        assert!(line["timestamp"].as_str().unwrap().ends_with('Z'));
        assert!(line.get("span").is_none(), "no span outside of one");
    }

    #[test]
    fn includes_current_span_fields_and_name() {
        let lines = capture_lines(k8s_format(), || {
            let span = tracing::info_span!(
                "request",
                request_id = "abc-123",
                method = %"POST",
                path = "/api/v1/x",
            );
            let _guard = span.enter();
            tracing::info!("inside");
        });

        let span = &lines[0]["span"];
        assert_eq!(span["name"], "request");
        assert_eq!(span["request_id"], "abc-123");
        assert_eq!(span["method"], "POST");
        assert_eq!(span["path"], "/api/v1/x");
    }

    #[test]
    fn omits_static_fields_when_none_configured() {
        let lines = capture_lines(K8sJson::with_fields(Vec::new()), || {
            tracing::warn!(err = %"boom", "something happened");
        });

        let line = &lines[0];
        assert_eq!(line["level"], "WARN");
        assert_eq!(line["err"], "boom");
        for key in ["pod", "namespace", "node", "service"] {
            assert!(line.get(key).is_none(), "{key} should be absent");
        }
    }
}
