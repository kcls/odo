//! Shared helpers for admin CRUD handlers.
//!
//! Every admin module follows the same conventions: trim-and-validate
//! string input (400 on bad input), map unique-constraint violations to
//! 409 conflicts with a stable error code, and treat empty optional
//! strings as NULL. Centralized here so the conventions are enforced by
//! code rather than imitation.

use odo_client::error::Error as LocalError;
use sea_orm::{ColumnTrait, DbErr, Order, SqlErr};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Default page size when a list request omits `limit`.
pub const DEFAULT_PAGE_LIMIT: u64 = 50;
/// Hard cap on page size so a client can't request an unbounded page.
pub const MAX_PAGE_LIMIT: u64 = 200;

/// Standard pagination inputs for admin list endpoints. Flatten this into a
/// per-resource request struct alongside that resource's own filter fields:
///
/// ```ignore
/// #[derive(Deserialize, ToSchema)]
/// pub struct ListThingsRequest {
///     #[serde(default)]
///     search: Option<String>,
///     #[serde(flatten)]
///     page: Page,
/// }
/// ```
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct Page {
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub offset: Option<u64>,
}

impl Page {
    /// Effective limit, clamped to `MAX_PAGE_LIMIT`.
    pub fn limit(&self) -> u64 {
        self.limit.unwrap_or(DEFAULT_PAGE_LIMIT).min(MAX_PAGE_LIMIT)
    }

    /// Effective offset (0 when unset).
    pub fn offset(&self) -> u64 {
        self.offset.unwrap_or(0)
    }
}

/// Standard admin list response: one page of rows plus the total count
/// matching the filter (ignoring limit/offset), so the UI can paginate.
#[derive(Debug, Serialize, ToSchema)]
pub struct Paginated<T> {
    pub rows: Vec<T>,
    pub total: i64,
}

impl<T> Paginated<T> {
    pub fn new(rows: Vec<T>, total: i64) -> Self {
        Self { rows, total }
    }
}

/// Sort direction for a list endpoint.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortDir {
    Asc,
    Desc,
}

impl From<SortDir> for Order {
    fn from(dir: SortDir) -> Self {
        match dir {
            SortDir::Asc => Order::Asc,
            SortDir::Desc => Order::Desc,
        }
    }
}

/// Standard sort inputs for admin list endpoints. Flatten this into a
/// per-resource request struct alongside `Page`:
///
/// ```ignore
/// #[derive(Deserialize, ToSchema)]
/// pub struct ListThingsRequest {
///     #[serde(default)]
///     search: Option<String>,
///     #[serde(flatten)]
///     page: Page,
///     #[serde(flatten)]
///     sort: Sort,
/// }
/// ```
///
/// `sort_by` is a client-supplied string, so a handler must NEVER map it to a
/// column directly — resolve it against an explicit allow-list with
/// [`Sort::resolve`], which falls back to the handler's default for any
/// unknown or absent key.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct Sort {
    /// Logical column key to sort by (must be one the handler allow-lists).
    #[serde(default)]
    pub sort_by: Option<String>,
    /// Sort direction; defaults to ascending when a `sort_by` is given.
    #[serde(default)]
    pub sort_dir: Option<SortDir>,
}

impl Sort {
    /// Resolve the requested sort against an allow-list of `(key, column)`
    /// pairs, returning the `(column, order)` to apply. Unknown/absent keys
    /// fall back to `default`. Callers should still append a stable tiebreaker
    /// (usually the primary key) so rows with equal sort values don't shuffle
    /// across page boundaries.
    ///
    /// ```ignore
    /// let (col, ord) = params.sort.resolve(
    ///     &[("code", Column::Code), ("description", Column::Description)],
    ///     (Column::Code, Order::Asc),
    /// );
    /// query.order_by(col, ord).order_by_asc(Column::Id)
    /// ```
    pub fn resolve<C: ColumnTrait>(
        &self,
        allowed: &[(&str, C)],
        default: (C, Order),
    ) -> (C, Order) {
        let Some(key) = self.sort_by.as_deref() else {
            return default;
        };
        match allowed.iter().find(|(k, _)| *k == key) {
            Some(&(_, col)) => {
                let dir = self.sort_dir.map(Order::from).unwrap_or(Order::Asc);
                (col, dir)
            }
            // Unknown key: ignore it rather than error, and use the default.
            None => default,
        }
    }
}

/// Declare a concrete, named page-response struct for one row type.
///
/// utoipa cannot emit a distinct, row-typed schema for a *generic*
/// `Paginated<T>` — a transparent `type XPage = Paginated<Row>` alias
/// collapses to a single un-parameterized `Paginated` schema whose `rows`
/// lose their element type in the generated OpenAPI (and therefore in the
/// generated TypeScript). So each list endpoint gets its own named struct:
///
/// ```ignore
/// page_type!(UnitTypePage, UnitTypeRow, "One page of org unit types.");
/// ```
///
/// The generated struct is identical in shape to `Paginated<Row>` and
/// converts from it, so handlers keep building `Paginated::new(rows, total)`
/// and return `Json(page.into())`.
#[macro_export]
macro_rules! page_type {
    ($name:ident, $row:ty, $doc:expr) => {
        #[doc = $doc]
        #[derive(Debug, serde::Serialize, utoipa::ToSchema)]
        pub struct $name {
            /// The rows on this page.
            pub rows: Vec<$row>,
            /// Total rows matching the filter, ignoring limit/offset.
            pub total: i64,
        }

        impl From<$crate::admin::Paginated<$row>> for $name {
            fn from(p: $crate::admin::Paginated<$row>) -> Self {
                Self {
                    rows: p.rows,
                    total: p.total,
                }
            }
        }
    };
}

/// Trimmed, non-empty string; 400 otherwise.
pub fn clean_required(value: &str, field: &str) -> Result<String, LocalError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(LocalError::invalid_input(format!("{field} may not be empty")));
    }
    Ok(value.to_string())
}

/// Identifier-style code: trimmed, non-empty, no whitespace, <= 100 chars.
pub fn clean_code(value: &str, field: &str) -> Result<String, LocalError> {
    let value = clean_required(value, field)?;
    if value.len() > 100 {
        return Err(LocalError::invalid_input(format!(
            "{field} may not exceed 100 characters"
        )));
    }
    if value.chars().any(char::is_whitespace) {
        return Err(LocalError::invalid_input(format!(
            "{field} may not contain whitespace"
        )));
    }
    Ok(value)
}

/// Trim; empty or missing becomes NULL.
pub fn clean_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(String::from)
}

/// Normalize a search term: trimmed, `None` if empty. Callers pair the
/// result with SeaORM's `Column::contains` (case-insensitive ILIKE on
/// Postgres for text columns).
pub fn clean_search(value: Option<&str>) -> Option<String> {
    clean_optional(value)
}

/// Plausible email address: trimmed, non-empty local and domain parts,
/// no whitespace, <= 255 chars.
pub fn clean_email(email: &str) -> Result<String, LocalError> {
    let email = clean_required(email, "email")?;
    if email.len() > 255 {
        return Err(LocalError::invalid_input("email may not exceed 255 characters"));
    }
    let valid = match email.split_once('@') {
        Some((local, domain)) => !local.is_empty() && !domain.is_empty(),
        None => false,
    };
    if !valid || email.chars().any(char::is_whitespace) {
        return Err(LocalError::invalid_input(format!("Invalid email address: {email}")));
    }
    Ok(email)
}

/// Map a unique-constraint violation to a 409 conflict with a stable error
/// code; anything else becomes a 500.
pub fn map_unique_violation(
    e: DbErr,
    code: &'static str,
    field: Option<&str>,
    message: &str,
) -> LocalError {
    if let Some(SqlErr::UniqueConstraintViolation(_)) = e.sql_err() {
        return LocalError::conflict(code, field, message);
    }
    LocalError::internal(e.to_string())
}
