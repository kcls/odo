// For helpers see https://docs.rs/handlebars/latest/handlebars/#custom-helper

pub use handlebars::RenderError;
use handlebars::template::{Template, TemplateElement};
use handlebars::{Handlebars, RenderErrorReason, handlebars_helper};
use odo_client::date;
use serde_json::Value;

handlebars_helper!(date_format_helper: |value: str, format: str, timezone: str| {
    let tz = if timezone.is_empty() { None } else { Some(timezone) };
    date::format_datetime_str(value, format, tz).unwrap_or_else(|_| value.to_string())
});

fn registry(escape: bool) -> Handlebars<'static> {
    let mut hbs = Handlebars::new();
    hbs.register_helper("date", Box::new(date_format_helper));
    if !escape {
        hbs.register_escape_fn(handlebars::no_escape);
    }
    hbs
}

/// Render a Handlebars template for **HTML** output: `{{var}}` values are
/// HTML-escaped, so variable data can't inject markup. Use for HTML email
/// bodies.
///
/// Handlebars' unescaped form, `{{{var}}}`, is refused with an error rather
/// than rendered: variable data comes from applications and can include
/// text typed by the public. Refusing it here, rather than only when a
/// template is saved, also covers templates that reach the database some
/// other way (registration, restored versions, direct SQL).
///
/// `template_text` is the user supplied template string and `context` is a
/// `serde_json::Value` containing the key/value pairs to substitute.
/// Returns the rendered string or an error describing what went wrong.
pub fn render(template_text: &str, context: &Value) -> Result<String, RenderError> {
    let template = Template::compile(template_text).map_err(RenderErrorReason::from)?;
    if has_unescaped_output(&template.elements) {
        return Err(RenderErrorReason::Other(
            "{{{...}}} (unescaped output) is not allowed; use {{...}}".to_string(),
        )
        .into());
    }
    registry(true).render_template(template_text, context)
}

/// Whether any `{{{...}}}` appears, including inside blocks (`{{#if}}`,
/// `{{#each}}`, their `{{else}}`s) and partial or decorator blocks.
fn has_unescaped_output(elements: &[TemplateElement]) -> bool {
    let nested = |t: &Option<Template>| {
        t.as_ref()
            .is_some_and(|t| has_unescaped_output(&t.elements))
    };
    elements.iter().any(|element| match element {
        TemplateElement::HtmlExpression(_) => true,
        TemplateElement::HelperBlock(h) => nested(&h.template) || nested(&h.inverse),
        TemplateElement::DecoratorBlock(d) | TemplateElement::PartialBlock(d) => {
            nested(&d.template)
        }
        _ => false,
    })
}

/// Render a Handlebars template for **plain-text** output (email subjects,
/// in-app titles and bodies, text email bodies): values are inserted as-is,
/// so `O'Brien` stays `O'Brien` rather than becoming `O&#x27;Brien`.
///
/// The result is not safe to put into HTML; pass it through
/// [`text_to_html`] first.
pub fn render_text(template_text: &str, context: &Value) -> Result<String, RenderError> {
    registry(false).render_template(template_text, context)
}

/// Plain text as an HTML fragment: everything escaped, line breaks kept.
/// For sending a text-only template as an HTML email.
pub fn text_to_html(text: &str) -> String {
    handlebars::html_escape(text)
        .replace("\r\n", "\n")
        .replace('\n', "<br>\n")
}

#[cfg(test)]
mod tests {
    use super::{render, render_text, text_to_html};
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
    fn render_escapes_values_for_html() {
        let context = json!({"name": "O'Brien & <Sons>"});
        let rendered = render("<p>{{name}}</p>", &context).expect("template renders");
        assert_eq!(rendered, "<p>O&#x27;Brien &amp; &lt;Sons&gt;</p>");
    }

    #[test]
    fn render_refuses_unescaped_output_anywhere() {
        let context = json!({"name": "<script>x</script>", "items": [1], "ok": true});
        for template in [
            "<p>{{{name}}}</p>",
            "<p>{{~{name}~}}</p>",
            "{{#if ok}}<p>{{{name}}}</p>{{/if}}",
            "{{#if ok}}fine{{else}}{{{name}}}{{/if}}",
            "{{#each items}}{{{../name}}}{{/each}}",
            "{{{date name \"%Y\" \"UTC\"}}}",
        ] {
            let err = render(template, &context).expect_err(template);
            assert!(err.to_string().contains("not allowed"), "{template}: {err}");
        }
    }

    #[test]
    fn render_allows_escaped_and_literal_braces() {
        let context = json!({"name": "Ann"});
        // A backslash-escaped tag and a raw block are template text, not data.
        assert_eq!(
            render(r"\{{{name}}} {{name}}", &context).unwrap(),
            "{{{name}}} Ann"
        );
        assert_eq!(
            render("{{{{raw}}}}{{{name}}}{{{{/raw}}}}", &context).unwrap(),
            "{{{name}}}"
        );
    }

    #[test]
    fn render_text_still_allows_triple_braces() {
        // Plain-text output never escapes, so {{{x}}} is the same as {{x}}.
        let context = json!({"name": "O'Brien"});
        assert_eq!(render_text("{{{name}}}", &context).unwrap(), "O'Brien");
    }

    #[test]
    fn render_text_inserts_values_verbatim() {
        let context = json!({"name": "O'Brien & <Sons>"});
        let rendered = render_text("Re: {{name}}", &context).expect("template renders");
        assert_eq!(rendered, "Re: O'Brien & <Sons>");
    }

    #[test]
    fn render_text_keeps_helpers() {
        let context = json!({"at": "2025-10-10T14:24:40+00:00"});
        let rendered =
            render_text("On {{date at \"%Y-%m-%d\" \"UTC\"}}", &context).expect("template renders");
        assert_eq!(rendered, "On 2025-10-10");
    }

    #[test]
    fn text_to_html_escapes_everything_and_keeps_line_breaks() {
        assert_eq!(
            text_to_html("Hi <b>Ann</b> & co\r\nline 2\nline 3"),
            "Hi &lt;b&gt;Ann&lt;/b&gt; &amp; co<br>\nline 2<br>\nline 3"
        );
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
