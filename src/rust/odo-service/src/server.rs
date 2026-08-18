use axum::Router;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto::Builder;
use std::time::Duration;
use tokio::net::TcpListener;
use tower::Service;
use tracing::error;

/// How often we ping clients to see if they are still connected.
const DEFAULT_H2_KEEPALIVE_SECS: u64 = 60;

fn h2_keepalive_interval() -> Duration {
    let secs: u64 = std::env::var("ODO_H2_KEEPALIVE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_H2_KEEPALIVE_SECS);
    Duration::from_secs(secs)
}

/// Serve an axum Router with HTTP/1.1 and h2c (HTTP/2 cleartext) support.
///
/// Dead connection detection: HTTP/2 PING frames at `ODO_H2_KEEPALIVE_SECS`
/// (default 30s) with a 10s response timeout.
pub async fn serve(listener: TcpListener, app: Router) -> std::io::Result<()> {
    let keepalive = h2_keepalive_interval();
    let shutdown = crate::signal::shutdown();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            result = listener.accept() => {
                let (stream, _addr) = result?;
                let tower_service = app.clone();

                tokio::spawn(async move {
                    let mut builder = Builder::new(TokioExecutor::new());
                    builder
                        .http2()
                        .keep_alive_interval(keepalive)
                        .keep_alive_timeout(Duration::from_secs(10))
                        .timer(TokioTimer::new());

                    let io = TokioIo::new(stream);
                    let hyper_service = hyper::service::service_fn(move |req| {
                        let mut svc = tower_service.clone();
                        async move { svc.call(req).await }
                    });

                    if let Err(err) = builder.serve_connection_with_upgrades(io, hyper_service).await
                        && !err.to_string().contains("connection closed") {
                            error!(error = %err, "connection error");
                        }
                });
            }
            _ = &mut shutdown => {
                tracing::info!("shutting down");
                break;
            }
        }
    }

    Ok(())
}
