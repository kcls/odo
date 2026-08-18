use tracing_subscriber::fmt::format::FmtSpan;

pub fn init(default_filter: &str) {
    tracing_subscriber::fmt()
        .json()
        .with_target(false)
        .with_file(true)
        .with_line_number(true)
        .with_current_span(true)
        .with_span_list(false)
        .flatten_event(true)
        .with_span_events(FmtSpan::NONE)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.parse().unwrap()),
        )
        .init();
}
