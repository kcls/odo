//! Client-side surface of the Odo platform: service clients, error and
//! request-context types, JWT verification, and date helpers. This is the
//! crate applications depend on to talk to Odo; it carries no server
//! scaffolding (see `odo-service`) and no database entities.

pub mod auth;
pub mod client;
pub mod context;
pub mod date;
pub mod error;
