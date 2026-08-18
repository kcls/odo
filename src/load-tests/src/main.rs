//! Load / soak test harness for the odo-* HTTP APIs.
//!
//! Runs a weighted mix of read operations (and, behind --writes,
//! self-cleaning write churn) against the gateway, then prints a
//! per-endpoint latency/error summary with an "areas of concern" section.
//!
//!   cargo run --release -- --workers 50 --duration 5m
//!   cargo run --release -- --mode open --rate 200 --duration 10m --ramp 1m
//!   cargo run --release -- --writes --workers 20 --duration 2m
//!
//! See README.md for the full knob list.

mod metrics;
mod ops;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use metrics::Sample;
use ops::{Ctx, Discovery, Op, READ_OPS, WRITE_OPS};
use rand::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
enum Mode {
    /// N workers in a request loop — "how does the system behave with N
    /// concurrent users?"
    Closed,
    /// Fixed arrival rate regardless of response times — "at what request
    /// rate does it fall over?"
    Open,
}

#[derive(Parser, Debug)]
#[command(about = "Load / soak tests for the odo-* APIs", version)]
struct Args {
    /// Gateway base URL (all odo routes hang off it)
    #[arg(long, env = "BASE_URL", default_value = "http://localhost:30080")]
    base_url: String,

    /// Concurrent workers (closed mode) / max in-flight requests (open mode)
    #[arg(long, short, default_value_t = 10)]
    workers: usize,

    /// Total run duration, e.g. 90s, 10m, 1h
    #[arg(long, short, default_value = "60s", value_parser = humantime::parse_duration)]
    duration: Duration,

    /// Ramp-up period at the start of the run (workers/rate scale in linearly)
    #[arg(long, default_value = "0s", value_parser = humantime::parse_duration)]
    ramp: Duration,

    #[arg(long, value_enum, default_value_t = Mode::Closed)]
    mode: Mode,

    /// Target request rate for --mode open (ops/sec; churn ops count once)
    #[arg(long, default_value_t = 50.0)]
    rate: f64,

    /// Enable the write-churn ops (create/delete cycles for roles,
    /// templates, org units, and asset directories). Leftovers are swept before
    /// and after the run.
    #[arg(long)]
    writes: bool,

    /// Progress/degradation window size in seconds
    #[arg(long, default_value_t = 10)]
    window: u64,

    /// p95 budget per op in milliseconds (exceeding it is flagged)
    #[arg(long, default_value_t = 800.0)]
    p95_budget_ms: f64,

    /// Flag degradation when last-quarter p95 exceeds first-quarter p95 by this factor
    #[arg(long, default_value_t = 1.5)]
    degradation_factor: f64,

    /// Exit non-zero if any concerns are flagged (for CI use)
    #[arg(long)]
    fail_on_concerns: bool,
}

/// Weighted op picker over a pre-expanded table.
struct Mix {
    table: Vec<&'static str>,
}

impl Mix {
    fn new(writes: bool) -> Self {
        let mut table = Vec::new();
        let ops: Vec<&Op> = READ_OPS
            .iter()
            .chain(writes.then_some(WRITE_OPS.iter()).into_iter().flatten())
            .collect();
        for op in ops {
            for _ in 0..op.weight {
                table.push(op.name);
            }
        }
        Self { table }
    }

    fn pick(&self, rng: &mut SmallRng) -> &'static str {
        self.table.choose(rng).expect("non-empty op table")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("odo load test");
    println!(
        "  target {}  mode {:?}  workers {}  duration {}  ramp {}  writes {}",
        args.base_url,
        args.mode,
        args.workers,
        humantime::format_duration(args.duration),
        humantime::format_duration(args.ramp),
        args.writes,
    );

    let ctx = Arc::new(
        Ctx::new(args.base_url.clone())
            .await
            .context("login failed — is the gateway reachable and e2e test data deployed?")?,
    );

    print!("  discovering data pools... ");
    let disc = Arc::new(ops::discover(&ctx).await.context("discovery failed")?);
    println!(
        "{} org units, {} users, {} permission codes",
        disc.unit_ids.len(),
        disc.user_ids.len(),
        disc.perm_codes.len()
    );

    if args.writes {
        let removed = ops::sweep_leftovers(&ctx).await.unwrap_or(0);
        if removed > 0 {
            println!("  swept {removed} leftover loadtest artifacts from a previous run");
        }
    }

    let mix = Arc::new(Mix::new(args.writes));
    let (tx, rx) = mpsc::unbounded_channel::<Sample>();
    let (stop_tx, stop_rx) = watch::channel(false);
    let start = Instant::now();

    let collector = tokio::spawn(metrics::Collector::new(rx, args.window.max(1)).run(start));

    let mut handles = Vec::new();
    match args.mode {
        Mode::Closed => {
            for i in 0..args.workers {
                let (ctx, disc, mix, tx, mut stop_rx) = (
                    ctx.clone(),
                    disc.clone(),
                    mix.clone(),
                    tx.clone(),
                    stop_rx.clone(),
                );
                // Stagger worker starts across the ramp window.
                let delay = if args.workers > 1 {
                    args.ramp.mul_f64(i as f64 / args.workers as f64)
                } else {
                    Duration::ZERO
                };
                handles.push(tokio::spawn(async move {
                    let mut rng = SmallRng::from_os_rng();
                    tokio::select! {
                        _ = tokio::time::sleep(delay) => {}
                        _ = stop_rx.wait_for(|s| *s) => return,
                    }
                    while !*stop_rx.borrow() {
                        let op = mix.pick(&mut rng);
                        timed_op(&ctx, &disc, op, &mut rng, &tx, start).await;
                    }
                }));
            }
        }
        Mode::Open => {
            // A single pacer spawns one op per tick; the semaphore caps
            // in-flight work at `workers` so a stalled backend can't pile
            // up unbounded tasks. Missed permits are counted as dropped.
            let sem = Arc::new(tokio::sync::Semaphore::new(args.workers));
            let (ctx, disc, mix, tx) = (ctx.clone(), disc.clone(), mix.clone(), tx.clone());
            let (ramp, rate) = (args.ramp, args.rate);
            let mut stop_rx = stop_rx.clone();
            handles.push(tokio::spawn(async move {
                let mut rng = SmallRng::from_os_rng();
                let mut dropped: u64 = 0;
                // Deadline-based pacing: schedule against an absolute clock so
                // per-iteration overhead can't erode the target rate.
                let mut next = tokio::time::Instant::now();
                while !*stop_rx.borrow() {
                    let elapsed = start.elapsed();
                    let factor = if ramp > Duration::ZERO && elapsed < ramp {
                        (elapsed.as_secs_f64() / ramp.as_secs_f64()).max(0.05)
                    } else {
                        1.0
                    };
                    let current = (rate * factor).max(0.1);

                    match sem.clone().try_acquire_owned() {
                        Ok(permit) => {
                            let (ctx, disc, tx) = (ctx.clone(), disc.clone(), tx.clone());
                            let op = mix.pick(&mut rng);
                            let mut task_rng = SmallRng::from_os_rng();
                            tokio::spawn(async move {
                                timed_op(&ctx, &disc, op, &mut task_rng, &tx, start).await;
                                drop(permit);
                            });
                        }
                        Err(_) => {
                            dropped += 1;
                            let _ = tx.send(Sample {
                                op: "(dropped: in-flight cap)",
                                at_secs: start.elapsed().as_secs(),
                                micros: 0,
                                status: 0,
                                ok: false,
                            });
                        }
                    }

                    next += Duration::from_secs_f64(1.0 / current);
                    // If we've fallen behind the schedule (long GC-ish stall),
                    // resync rather than bursting to catch up.
                    let now = tokio::time::Instant::now();
                    if next < now {
                        next = now;
                    }
                    tokio::select! {
                        _ = tokio::time::sleep_until(next) => {}
                        _ = stop_rx.wait_for(|s| *s) => break,
                    }
                }
                if dropped > 0 {
                    eprintln!("  ({dropped} ticks dropped at the in-flight cap)");
                }
            }));
        }
    }
    drop(tx); // collector finishes once all worker senders drop

    // Run for the configured duration, or until Ctrl-C.
    tokio::select! {
        _ = tokio::time::sleep(args.duration) => {}
        _ = tokio::signal::ctrl_c() => println!("  interrupted — finishing up"),
    }
    let _ = stop_tx.send(true);
    for h in handles {
        let _ = h.await;
    }

    if args.writes {
        let removed = ops::sweep_leftovers(&ctx).await.unwrap_or(0);
        if removed > 0 {
            println!("  swept {removed} loadtest artifacts after the run");
        }
    }

    let stats = collector.await.context("collector task")?;
    let concerns = metrics::print_report(
        &stats,
        &metrics::ReportConfig {
            p95_budget_ms: args.p95_budget_ms,
            degradation_factor: args.degradation_factor,
            min_samples: 20,
            skip_ramp_windows: args.ramp.as_secs().div_ceil(args.window.max(1)),
            // The ramp period runs below the target by design; compare the
            // achieved average against the ramp-discounted expectation.
            target_rate: (args.mode == Mode::Open).then_some(
                args.rate * (args.duration.as_secs_f64() - args.ramp.as_secs_f64() / 2.0).max(1.0)
                    / args.duration.as_secs_f64().max(1.0),
            ),
            token_refreshes: ctx.token_refreshes(),
            workers: args.workers,
        },
    );

    if args.fail_on_concerns && !concerns.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

/// Execute one op and forward each HTTP call's own timing to the collector.
async fn timed_op(
    ctx: &Ctx,
    disc: &Discovery,
    op: &'static str,
    rng: &mut SmallRng,
    tx: &mpsc::UnboundedSender<Sample>,
    start: Instant,
) {
    for (name, result) in ops::execute(ctx, disc, op, rng).await {
        let _ = tx.send(Sample {
            op: name,
            at_secs: start.elapsed().as_secs(),
            micros: result.micros,
            status: result.status,
            ok: result.ok,
        });
    }
}
