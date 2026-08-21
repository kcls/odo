//! Authorization helpers: permission and role checks against the database.
//!
//! These functions are used both internally (e.g. login's auth.session check)
//! and exposed as HTTP endpoints for other services.

use odo_client::error::LocalResult;
use odo_entity::authz::usr_role_org_map;
use sea_orm::prelude::*;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, QueryOrder, Statement};

/// Check if a user has a specific permission at an optional org unit.
/// If org_unit is None, checks against the root org unit.
pub async fn user_has_perm(
    db: &DatabaseConnection,
    user_id: i32,
    perm: &str,
    org_unit: Option<i32>,
) -> LocalResult<bool> {
    tracing::info!("Checking user {user_id} has permission {perm}");

    let result = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT authz.usr_has_perm_at($1, $2, $3) AS has_perm",
            [user_id.into(), perm.into(), org_unit.into()],
        ))
        .await?
        .and_then(|r| r.try_get("", "has_perm").ok())
        .unwrap_or(false);

    Ok(result)
}

/// Check if a user has a specific role at an optional org unit.
pub async fn user_has_role(
    db: &DatabaseConnection,
    user_id: i32,
    role: &str,
    org_unit: Option<i32>,
) -> LocalResult<bool> {
    tracing::info!("Checking user {user_id} has role {role}");

    let result = if let Some(ou) = org_unit {
        db.query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT authz.usr_has_role_at($1, $2, $3) AS has_role",
            [user_id.into(), role.into(), ou.into()],
        ))
        .await?
        .and_then(|r| r.try_get("", "has_role").ok())
        .unwrap_or(false)
    } else {
        db.query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"
            SELECT EXISTS(
                SELECT 1 FROM authz.usr_role_org_map
                WHERE usr = $1 AND role = $2
                LIMIT 1
            ) AS has_role
            "#,
            [user_id.into(), role.into()],
        ))
        .await?
        .and_then(|r| r.try_get("", "has_role").ok())
        .unwrap_or(false)
    };

    Ok(result)
}

/// Filter `user_ids` down to those who hold `role`.
///
/// Semantics match [`user_has_role`], applied to each candidate user.
/// Role grants propagate from the granting org unit down to all of its
/// descendants; the check succeeds when the user has a grant at the
/// target unit or at any of its ancestors.
///
/// - `org_unit = Some(id)`: check at that target unit (with ancestor
///   propagation).
/// - `org_unit = None`: defer to `authz.usr_has_role_at` with NULL,
///   which falls back to the root org unit. Because grants at root
///   propagate everywhere, this effectively asks "has the role been
///   granted at root" — *not* "granted anywhere in the tree".
///
/// Single round-trip regardless of input size — use this to avoid N+1
/// `user_has_role` calls when checking role membership for a known set
/// of users.
pub async fn users_with_role(
    db: &DatabaseConnection,
    user_ids: &[i32],
    role: &str,
    org_unit: Option<i32>,
) -> LocalResult<Vec<i32>> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }

    let ids_vec: Vec<i32> = user_ids.to_vec();

    // Single SQL form: pass org_unit through (NULL handled by usr_has_role_at).
    let stmt = Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"
        SELECT usr
        FROM unnest($1::int[]) AS u(usr)
        WHERE authz.usr_has_role_at(u.usr, $2, $3)
        "#,
        [ids_vec.into(), role.into(), org_unit.into()],
    );

    let rows = db.query_all_raw(stmt).await?;
    let matched: Vec<i32> = rows
        .iter()
        .filter_map(|r| r.try_get::<i32>("", "usr").ok())
        .collect();

    Ok(matched)
}

/// Get all roles for a user as (role, org_unit) pairs.
pub async fn get_user_roles(
    db: &DatabaseConnection,
    user_id: i32,
) -> LocalResult<Vec<(String, i32)>> {
    let rows = usr_role_org_map::Entity::find()
        .filter(usr_role_org_map::Column::Usr.eq(user_id))
        .order_by_asc(usr_role_org_map::Column::OrgUnit)
        .order_by_asc(usr_role_org_map::Column::Role)
        .all(db)
        .await?;

    Ok(rows.into_iter().map(|r| (r.role, r.org_unit)).collect())
}
