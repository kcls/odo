// For helpers see https://docs.rs/handlebars/latest/handlebars/#custom-helper

use handlebars::{Handlebars, handlebars_helper};
use odo_client::date;
use serde_json::Value;

handlebars_helper!(date_format_helper: |value: str, format: str, timezone: str| {
    let tz = if timezone.is_empty() { None } else { Some(timezone) };
    date::format_datetime_str(value, format, tz).unwrap_or_else(|_| value.to_string())
});

/// Render a Handlebars template with the provided context.
///
/// `template_text` is the user supplied template string and `context` is a
/// `serde_json::Value` containing the key/value pairs to substitute.
/// Returns the rendered string or an error describing what went wrong.
pub fn render(template_text: &str, context: &Value) -> Result<String, handlebars::RenderError> {
    let mut hbs = Handlebars::new();
    hbs.register_helper("date", Box::new(date_format_helper));
    hbs.render_template(template_text, context)
}

#[cfg(test)]
mod tests {
    use super::render;
    use serde_json::json;

    #[test]
    fn renders_template_with_variables() {
        let template = "Hello {{first_name}} {{last_name}}, welcome to {{company}}!";
        let context = json!({
            "first_name": "Jane",
            "last_name": "Doe",
            "company": "Odo"
        });

        let rendered = render(template, &context).expect("template renders");
        assert_eq!(rendered, "Hello Jane Doe, welcome to Odo!");
    }

    #[test]
    fn formats_date_helper() {
        let template =
            "Incident date: {{date incident_created_at \"%Y-%m-%d\" \"America/New_York\"}}";
        let context = json!({
            "incident_created_at": "2025-10-10T14:24:40.450279+00:00"
        });

        let rendered = render(template, &context).expect("template renders");
        assert_eq!(rendered, "Incident date: 2025-10-10");
    }
}
