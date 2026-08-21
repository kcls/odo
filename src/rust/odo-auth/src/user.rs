use axum::Json;
use axum::extract::State;
use odo_client::error::LocalError;
use odo_entity::auth::{saml_usr_working_location, usr};
use odo_entity::org::unit as org_unit_entity;
use sea_orm::prelude::*;
use sea_orm::sea_query::{Expr, extension::postgres::PgExpr};
use sea_orm::{Condition, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};
use std::cmp;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;
use crate::authz;
use odo_client::error::ApiResult;

const MAX_QUERY_RESULTS: u64 = 1000;

#[derive(Debug, Serialize, ToSchema)]
pub struct UserResponse {
    pub id: i32,
    /// Stable, DB-independent identity (see odo-durable-references).
    pub uuid: String,
    pub email: String,
    pub username: Option<String>,
    pub first_given_name: Option<String>,
    pub second_given_name: Option<String>,
    pub family_name: Option<String>,
    pub display_name: String,
    pub status: Option<String>,
    /// RFC3339 soft-delete timestamp; null for active users. get_user resolves
    /// by id and returns soft-deleted users flagged so historical `*_by`
    /// references still render (login/search stay active-only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_org_units: Option<Vec<i32>>,
    /// Stable uuids of the working org units, parallel to
    /// `working_org_units` (uuid migration).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_org_unit_uuids: Option<Vec<String>>,
    // Avoid returning additional account metadata (e.g. last_login_at)
    // for this API.  Such data may require augmented permissions.
}

impl From<usr::Model> for UserResponse {
    fn from(u: usr::Model) -> Self {
        Self {
            id: u.id,
            uuid: u.uuid.to_string(),
            email: u.email,
            username: u.username,
            first_given_name: u.first_given_name,
            second_given_name: u.second_given_name,
            family_name: u.family_name,
            display_name: u.display_name,
            status: u.status.map(|s| format!("{s:?}").to_lowercase()),
            deleted_at: u.deleted_at.map(|d| d.to_rfc3339()),
            working_org_units: None,
            working_org_unit_uuids: None,
        }
    }
}

#[derive(Debug, Default, Deserialize, ToSchema)]
struct GetUserOptions {
    #[serde(default)]
    with_working_locations: bool,
    /// Opt in to resolve a soft-deleted user (flagged with deleted_at) instead
    /// of 404. Default false keeps get_user active-only.
    #[serde(default)]
    with_deleted: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GetUserRequest {
    #[serde(default)]
    id: Option<i32>,
    /// Resolve by stable uuid instead of database id (durable references).
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    options: Option<GetUserOptions>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UserSearchRequest {
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    offset: Option<u64>,
    status: Option<String>,
    keywords: Option<String>,
    #[serde(default)]
    options: Option<GetUserOptions>,
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/user/get",
    request_body = GetUserRequest,
    responses((status = 200, body = UserResponse, description = "User profile")),
    security(("bearer" = [])),
    tag = "user"
)]
pub async fn get_user(
    State(state): State<Arc<AppState>>,
    Json(params): Json<GetUserRequest>,
) -> ApiResult<Json<UserResponse>> {
    let caller_id =
        odo_client::context::RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;

    let options = params.options.unwrap_or_default();

    // Resolve a uuid to the database id first (durable references): uuid
    // lookups follow the same permission rule as id lookups.
    let user_id = if let Some(uuid) = params.uuid {
        let uuid: Uuid = uuid
            .parse()
            .map_err(|_| LocalError::invalid_input("invalid uuid"))?;
        let mut query = usr::Entity::find().filter(usr::Column::Uuid.eq(uuid));
        if !options.with_deleted {
            query = query.filter(usr::Column::DeletedAt.is_null());
        }
        let row = query
            .one(&state.db)
            .await?
            .ok_or_else(|| LocalError::not_found(format!("user uuid {uuid}")))?;
        row.id
    } else {
        params.id.unwrap_or(caller_id)
    };

    if user_id != caller_id
        && !authz::user_has_perm(&state.db, caller_id, "odo.auth.user.read", None).await?
    {
        return Err(LocalError::permission_denied("odo.auth.user.read", None).into());
    }

    let user = fetch_user(&state.db, user_id, &options).await?;

    Ok(Json(user))
}

async fn fetch_user(
    db: &DatabaseConnection,
    user_id: i32,
    options: &GetUserOptions,
) -> ApiResult<UserResponse> {
    // Active-only by default; opt in (with_deleted) to resolve a soft-deleted
    // user (flagged via deleted_at) so historical *_by references still render.
    // (Login and user search always filter deleted.)
    let mut query = usr::Entity::find_by_id(user_id);
    if !options.with_deleted {
        query = query.filter(usr::Column::DeletedAt.is_null());
    }
    let model = query
        .one(db)
        .await?
        .ok_or(LocalError::not_found(format!("user id={user_id}")))?;

    let mut user = UserResponse::from(model);

    if options.with_working_locations {
        let (ids, uuids) = fetch_working_locations(db, user_id).await?;
        user.working_org_units = Some(ids);
        user.working_org_unit_uuids = Some(uuids);
    }

    Ok(user)
}

async fn fetch_working_locations(
    db: &DatabaseConnection,
    user_id: i32,
) -> ApiResult<(Vec<i32>, Vec<String>)> {
    let ids: Vec<i32> = saml_usr_working_location::Entity::find()
        .filter(saml_usr_working_location::Column::Ident.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|l| l.org_unit)
        .collect();

    // Resolve the parallel uuid list in one query, preserving id order.
    let units = org_unit_entity::Entity::find()
        .filter(org_unit_entity::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await?;
    let by_id: std::collections::HashMap<i32, String> = units
        .into_iter()
        .map(|u| (u.id, u.uuid.to_string()))
        .collect();
    let uuids = ids.iter().filter_map(|id| by_id.get(id).cloned()).collect();

    Ok((ids, uuids))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/user/search",
    request_body = UserSearchRequest,
    responses((status = 200, body = Vec<UserResponse>, description = "Search results")),
    security(("bearer" = [])),
    tag = "user"
)]
pub async fn user_search(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UserSearchRequest>,
) -> ApiResult<Json<Vec<UserResponse>>> {
    let caller_id =
        odo_client::context::RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;

    if !authz::user_has_perm(&state.db, caller_id, "odo.auth.user.read", None).await? {
        return Err(LocalError::permission_denied("odo.auth.user.read", None).into());
    }

    let limit = cmp::min(params.limit.unwrap_or(10), MAX_QUERY_RESULTS);
    let offset = params.offset.unwrap_or(0);
    let status = params.status.as_deref().unwrap_or("active");

    let keywords = params.keywords.as_deref().unwrap_or("").replace('%', "");
    let terms: Vec<&str> = keywords.split_whitespace().collect();

    if terms.is_empty() {
        return Err(LocalError::invalid_input("Search keywords required").into());
    }

    // Compose the WHERE clause programmatically. Each search term becomes
    // an OR-group of three ILIKE clauses (display_name, email, username),
    // and the resulting per-term groups are AND'd together. SeaORM's
    // .like() emits LIKE (case-sensitive); we use the Postgres `ilike`
    // extension trait for case-insensitive matching.
    let mut where_cond = Condition::all()
        .add(usr::Column::DeletedAt.is_null())
        .add(usr::Column::Status.eq(status));

    for term in &terms {
        let match_pat = format!("%{term}%");
        let prefix_pat = format!("{term}%");
        where_cond = where_cond.add(
            Condition::any()
                .add(Expr::col(usr::Column::DisplayName).ilike(match_pat.clone()))
                .add(Expr::col(usr::Column::Email).ilike(match_pat))
                .add(Expr::col(usr::Column::Username).ilike(prefix_pat)),
        );
    }

    let users = usr::Entity::find()
        .filter(where_cond)
        .order_by_asc(usr::Column::DisplayName)
        .limit(limit)
        .offset(offset)
        .all(&state.db)
        .await?;

    let options = params.options.unwrap_or_default();
    let mut user_list = Vec::new();

    for model in users {
        let mut user = UserResponse::from(model);
        if options.with_working_locations {
            let (ids, uuids) = fetch_working_locations(&state.db, user.id).await?;
            user.working_org_units = Some(ids);
            user.working_org_unit_uuids = Some(uuids);
        }
        user_list.push(user);
    }

    Ok(Json(user_list))
}
