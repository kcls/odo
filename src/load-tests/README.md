# odo load tests

Load / soak test harness for the odo-* HTTP APIs (auth, org, notify,
asset). Runs a weighted mix of API operations through the gateway,
prints live per-window progress, and finishes with a per-endpoint
latency/error summary plus an **Areas of concern** section.

Distinct from `src/integration-tests` on purpose: those are
single-threaded correctness tests; this hammers the system concurrently
and only ever asserts statistics.

## Quick start

```bash
cd src/load-tests

# 10 workers for 60 seconds against the local dev gateway
cargo run --release

# 50 concurrent workers for 5 minutes, 1 minute ramp-up
cargo run --release -- --workers 50 --duration 5m --ramp 1m

# fixed-arrival-rate run: 200 ops/sec for 10 minutes
cargo run --release -- --mode open --rate 200 --duration 10m --ramp 1m

# include the write-churn ops (see below)
cargo run --release -- --writes --workers 20 --duration 2m

# point at another environment
BASE_URL=https://current-stage.example.org cargo run --release -- --workers 25 --duration 5m
```

Requires the e2e test users (`e2e.odo.admin` / `test123!`)
to exist in the target environment — the same seed data the integration
tests use.

## Knobs

| Flag | Default | Meaning |
|---|---|---|
| `--base-url` / `BASE_URL` | `http://localhost:30080` | Gateway base URL |
| `--workers` / `-w` | 10 | Concurrent workers (closed mode); max in-flight (open mode) |
| `--duration` / `-d` | 60s | Total run length (`90s`, `10m`, `1h`) |
| `--ramp` | 0s | Ramp-up period — workers/rate scale in linearly |
| `--mode` | closed | `closed` = N workers looping; `open` = fixed arrival rate |
| `--rate` | 50 | Target ops/sec for open mode |
| `--writes` | off | Enable the write-churn ops |
| `--window` | 10 | Progress/degradation window seconds |
| `--p95-budget-ms` | 800 | Per-op p95 above this is flagged |
| `--degradation-factor` | 1.5 | Flag when last-quarter p95 exceeds first-quarter p95 by this factor |
| `--fail-on-concerns` | off | Exit non-zero if anything was flagged (CI) |

**Closed vs open**: closed mode answers "how does the system behave with
N concurrent users?" — when the system slows down, the offered load
slows with it. Open mode answers "at what request rate does it fall
over?" — requests arrive on schedule regardless of response times, so
saturation shows up as queueing latency and (past the in-flight cap)
dropped ticks.

## What it calls

Nothing is hard-coded to seed data: at startup the harness logs in and
*discovers* its data pools — the org unit tree, unit types, user ids,
username-derived search terms, and the live permission codes — then
samples from them. The read mix covers every odo service:

- **odo-auth**: `authz/user-has-perm` (random real perm × random unit),
  `user/search`, `user/get`, `authz/role/list`, `authz/permission/list`
- **odo-org**: `unit/{id}`, `unit/{id}/descendants`, `unit/{id}/ancestors`,
  `unit/label-batch`, `tree`, `root`
- **odo-notify**: `inbox/list`, `email-group/list`, `template/list`
- **odo-asset**: `api-doc` (read-only mode has no way to discover file
  ids; full asset coverage comes from `--writes`)

`--writes` adds self-cleaning churn cycles, each step reported as its
own op:

- **auth**: role create → delete
- **notify**: template create → delete
- **org**: unit create under root → delete (low weight)
- **asset**: upload → files/get → retrieve → delete (this is the real
  asset read coverage)

All write artifacts are prefixed `loadtest-`/`LoadTest`, and leftovers
from crashed runs are swept before and after every `--writes` session.
Note that soft-deleting churn rows still grows the underlying tables
over very long runs.

## Reading the report

Each op gets count, rate, error %, and p50/p95/p99/max latency. The
concerns section flags, per op (minimum 20 samples):

- error rate > 1% (with a status-code breakdown), or any
  transport-level failures (connection reset / timeout)
- p95 over the budget
- **degradation**: last-quarter p95 exceeding first-quarter p95 by the
  configured factor — the "it gets slower the longer it runs" signal
  averages hide (leaks, pool exhaustion)

Run-level flags: achieved rate more than 10% below the (ramp-adjusted)
open-mode target, and excessive token re-logins.

Client-side numbers only see half the story — during a long run, keep
an eye on `kubectl top pods` and pod restart counts on the target
cluster.

## Caveats

- Against the single-box dev cluster you're measuring envoy + services +
  the shared Postgres together; treat results as relative comparisons,
  not absolute capacity. Point `BASE_URL` at stage for realistic numbers.
- The harness process itself needs headroom: at several thousand req/s,
  run it `--release` (always) and watch that the load box isn't the
  bottleneck (the open-mode achieved-rate concern will hint at this).
