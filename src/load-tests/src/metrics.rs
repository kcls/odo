//! Sample collection and end-of-run reporting.
//!
//! Workers send one [`Sample`] per request over an unbounded channel; a
//! single collector task owns all the histograms (no lock contention on
//! the hot path). Latencies are kept both per-op for the whole run and
//! per-op per time window, so the report can detect degradation over the
//! course of a sustained run — a climbing p95 that a whole-run average
//! would hide.

use hdrhistogram::Histogram;
use std::collections::BTreeMap;
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct Sample {
    /// Static op name, e.g. "org.unit_detail".
    pub op: &'static str,
    /// Seconds since the run started (used for windowing).
    pub at_secs: u64,
    pub micros: u64,
    /// HTTP status; 0 for transport-level failures.
    pub status: u16,
    pub ok: bool,
}

fn new_hist() -> Histogram<u64> {
    // 1µs .. 5min, 3 significant digits.
    Histogram::new_with_bounds(1, 300_000_000, 3).expect("static histogram bounds")
}

#[derive(Debug)]
pub struct OpStats {
    pub hist: Histogram<u64>,
    pub errors: u64,
    pub transport_errors: u64,
    /// status -> count for non-2xx responses.
    pub error_statuses: BTreeMap<u16, u64>,
    /// window index -> latency histogram (successful requests only, so a
    /// flood of fast error responses can't mask a latency climb).
    pub windows: BTreeMap<u64, Histogram<u64>>,
}

impl OpStats {
    fn new() -> Self {
        Self {
            hist: new_hist(),
            errors: 0,
            transport_errors: 0,
            error_statuses: BTreeMap::new(),
            windows: BTreeMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct RunStats {
    pub ops: BTreeMap<&'static str, OpStats>,
    pub total: u64,
    pub total_errors: u64,
    pub wall_secs: f64,
}

impl RunStats {
    pub fn total_rps(&self) -> f64 {
        if self.wall_secs > 0.0 {
            self.total as f64 / self.wall_secs
        } else {
            0.0
        }
    }
}

pub struct Collector {
    rx: mpsc::UnboundedReceiver<Sample>,
    window_secs: u64,
    ops: BTreeMap<&'static str, OpStats>,
    total: u64,
    total_errors: u64,
    /// Rolling counters for the live progress line.
    window_count: u64,
    window_errors: u64,
    window_hist: Histogram<u64>,
    current_window: u64,
}

impl Collector {
    pub fn new(rx: mpsc::UnboundedReceiver<Sample>, window_secs: u64) -> Self {
        Self {
            rx,
            window_secs,
            ops: BTreeMap::new(),
            total: 0,
            total_errors: 0,
            window_count: 0,
            window_errors: 0,
            window_hist: new_hist(),
            current_window: 0,
        }
    }

    /// Drain samples until every sender is dropped, printing one progress
    /// line per window. Returns the aggregated stats.
    pub async fn run(mut self, wall_start: std::time::Instant) -> RunStats {
        while let Some(sample) = self.rx.recv().await {
            self.ingest(sample);
        }
        self.flush_progress();
        RunStats {
            ops: self.ops,
            total: self.total,
            total_errors: self.total_errors,
            wall_secs: wall_start.elapsed().as_secs_f64(),
        }
    }

    fn ingest(&mut self, s: Sample) {
        let window = s.at_secs / self.window_secs;
        if window != self.current_window {
            self.flush_progress();
            self.current_window = window;
        }

        self.total += 1;
        self.window_count += 1;
        let op = self.ops.entry(s.op).or_insert_with(OpStats::new);
        if s.ok {
            op.hist.saturating_record(s.micros);
            op.windows
                .entry(window)
                .or_insert_with(new_hist)
                .saturating_record(s.micros);
            self.window_hist.saturating_record(s.micros);
        } else {
            op.errors += 1;
            self.total_errors += 1;
            self.window_errors += 1;
            if s.status == 0 {
                op.transport_errors += 1;
            }
            *op.error_statuses.entry(s.status).or_default() += 1;
        }
    }

    fn flush_progress(&mut self) {
        if self.window_count == 0 {
            return;
        }
        let p95 = self.window_hist.value_at_quantile(0.95) as f64 / 1000.0;
        let rps = self.window_count as f64 / self.window_secs as f64;
        println!(
            "  t+{:>4}s  {:>7.1} req/s  errors {:>4}  p95 {:>8.1}ms",
            (self.current_window + 1) * self.window_secs,
            rps,
            self.window_errors,
            p95,
        );
        self.window_count = 0;
        self.window_errors = 0;
        self.window_hist = new_hist();
    }
}

fn ms(hist: &Histogram<u64>, q: f64) -> f64 {
    hist.value_at_quantile(q) as f64 / 1000.0
}

/// One flagged problem for the "Areas of concern" section.
pub struct Concern {
    pub op: String,
    pub message: String,
}

pub struct ReportConfig {
    pub p95_budget_ms: f64,
    /// Flag when the last quarter's p95 exceeds first quarter's by this factor.
    pub degradation_factor: f64,
    /// Minimum samples before an op's error-rate/latency is judged.
    pub min_samples: u64,
    /// Windows to exclude from the degradation baseline (the ramp period
    /// runs under lighter load, which would fake a degradation signal).
    pub skip_ramp_windows: u64,
    /// Open-mode target rate, when applicable.
    pub target_rate: Option<f64>,
    pub token_refreshes: u64,
    pub workers: usize,
}

pub fn print_report(stats: &RunStats, cfg: &ReportConfig) -> Vec<Concern> {
    println!();
    println!("================================= SUMMARY =================================");
    println!(
        "  {:.0}s wall, {} requests ({:.1} req/s), {} errors ({:.2}%)",
        stats.wall_secs,
        stats.total,
        stats.total_rps(),
        stats.total_errors,
        if stats.total > 0 {
            100.0 * stats.total_errors as f64 / stats.total as f64
        } else {
            0.0
        },
    );
    println!();
    println!(
        "  {:<28} {:>8} {:>7} {:>6} {:>9} {:>9} {:>9} {:>9}",
        "op", "count", "req/s", "err%", "p50 ms", "p95 ms", "p99 ms", "max ms"
    );
    println!("  {}", "-".repeat(94));

    let mut concerns: Vec<Concern> = Vec::new();

    for (name, op) in &stats.ops {
        let count = op.hist.len() + op.errors;
        let err_pct = if count > 0 {
            100.0 * op.errors as f64 / count as f64
        } else {
            0.0
        };
        println!(
            "  {:<28} {:>8} {:>7.1} {:>6.2} {:>9.1} {:>9.1} {:>9.1} {:>9.1}",
            name,
            count,
            count as f64 / stats.wall_secs.max(0.001),
            err_pct,
            ms(&op.hist, 0.50),
            ms(&op.hist, 0.95),
            ms(&op.hist, 0.99),
            op.hist.max() as f64 / 1000.0,
        );

        if count < cfg.min_samples {
            continue;
        }

        if err_pct > 1.0 {
            let statuses: Vec<String> = op
                .error_statuses
                .iter()
                .map(|(s, n)| {
                    if *s == 0 {
                        format!("transport x{n}")
                    } else {
                        format!("{s} x{n}")
                    }
                })
                .collect();
            concerns.push(Concern {
                op: name.to_string(),
                message: format!("error rate {err_pct:.2}% ({})", statuses.join(", ")),
            });
        } else if op.transport_errors > 0 {
            concerns.push(Concern {
                op: name.to_string(),
                message: format!(
                    "{} transport-level failures (connection reset/timeout)",
                    op.transport_errors
                ),
            });
        }

        let p95 = ms(&op.hist, 0.95);
        if p95 > cfg.p95_budget_ms {
            concerns.push(Concern {
                op: name.to_string(),
                message: format!("p95 {p95:.0}ms exceeds budget {:.0}ms", cfg.p95_budget_ms),
            });
        }

        // Degradation: compare the first and last quarters of the run's
        // windows. Needs enough samples on both ends to mean anything.
        if let Some((&earliest, _)) = op.windows.first_key_value()
            && let Some((&last_w, _)) = op.windows.last_key_value()
            && let first_w = earliest.max(cfg.skip_ramp_windows)
            && last_w > first_w + 3
        {
            let span = last_w - first_w + 1;
            let quarter = (span / 4).max(1);
            let merged = |range: std::ops::RangeInclusive<u64>| {
                let mut h = new_hist();
                for (_, wh) in op.windows.range(range) {
                    h.add(wh).expect("identical histogram bounds");
                }
                h
            };
            let head = merged(first_w..=first_w + quarter - 1);
            let tail = merged(last_w + 1 - quarter..=last_w);
            if head.len() >= cfg.min_samples && tail.len() >= cfg.min_samples {
                let (h95, t95) = (ms(&head, 0.95), ms(&tail, 0.95));
                if t95 > h95 * cfg.degradation_factor && t95 - h95 > 20.0 {
                    concerns.push(Concern {
                        op: name.to_string(),
                        message: format!(
                            "latency degraded over the run: p95 {h95:.0}ms (first quarter) -> {t95:.0}ms (last quarter)"
                        ),
                    });
                }
            }
        }
    }

    // Run-level checks.
    if let Some(target) = cfg.target_rate {
        let achieved = stats.total_rps();
        if achieved < target * 0.9 {
            concerns.push(Concern {
                op: "(run)".to_string(),
                message: format!(
                    "achieved {achieved:.1} req/s vs target {target:.1} — the system (or the harness box) is saturated below the requested rate"
                ),
            });
        }
    }
    if cfg.token_refreshes > cfg.workers as u64 {
        concerns.push(Concern {
            op: "(auth)".to_string(),
            message: format!(
                "{} token re-logins during the run — sessions expiring or being invalidated under load",
                cfg.token_refreshes
            ),
        });
    }

    println!();
    if concerns.is_empty() {
        println!("Areas of concern: none — all ops within error, latency, and stability budgets.");
    } else {
        println!("Areas of concern ({}):", concerns.len());
        for c in &concerns {
            println!("  ! {:<28} {}", c.op, c.message);
        }
    }
    println!("===========================================================================");
    concerns
}
