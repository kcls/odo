//! Server-side scaffold shared by the odo-* services (and available to
//! applications as a convenience): axum server bootstrap, auth middleware,
//! logging, health checks, openapi glue, templating, and admin-list
//! helpers. The client-facing surface (service clients, error/context
//! types, JWT verification) lives in `odo-client`; database entities live
//! in `odo-entity`, private to the odo services.

pub mod admin;
pub mod db;
pub mod health;
pub mod logging;
pub mod middleware;
pub mod openapi;
pub mod server;
pub mod signal;
pub use hyper;
pub use hyper_util;
pub mod template;

pub use sea_orm;
