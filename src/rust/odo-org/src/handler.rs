use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use odo_entity::org::{address, closure, operating_hours, unit, unit_type};
use odo_client::error::{ApiResult, LocalError};
use sea_orm::*;
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct AddressResponse {
    pub id: i32,
    pub org_unit: i32,
    pub address_type: String,
    pub label: String,
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub city: Option<String>,
    pub state_province: Option<String>,
    pub postal_code: Option<String>,
}

impl From<address::Model> for AddressResponse {
    fn from(m: address::Model) -> Self {
        Self {
            id: m.id,
            org_unit: m.org_unit,
            address_type: m.address_type,
            label: m.label,
            address_line1: m.address_line1,
            address_line2: m.address_line2,
            city: m.city,
            state_province: m.state_province,
            postal_code: m.postal_code,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OperatingHoursResponse {
    pub id: i32,
    pub org_unit: i32,
    pub day_of_week: i32,
    pub open_time: chrono::NaiveTime,
    pub close_time: chrono::NaiveTime,
    pub is_closed: Option<bool>,
}

impl From<operating_hours::Model> for OperatingHoursResponse {
    fn from(m: operating_hours::Model) -> Self {
        Self {
            id: m.id,
            org_unit: m.org_unit,
            day_of_week: m.day_of_week,
            open_time: m.open_time,
            close_time: m.close_time,
            is_closed: m.is_closed,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ClosureResponse {
    pub id: i32,
    pub org_unit: i32,
    pub closure_date: chrono::NaiveDate,
    pub reason: String,
    pub is_emergency: Option<bool>,
}

impl From<closure::Model> for ClosureResponse {
    fn from(m: closure::Model) -> Self {
        Self {
            id: m.id,
            org_unit: m.org_unit,
            closure_date: m.closure_date,
            reason: m.reason,
            is_emergency: m.is_emergency,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrgUnitDetailResponse {
    pub org_unit: OrgUnitResponse,
    pub addresses: Vec<AddressResponse>,
    pub operating_hours: Vec<OperatingHoursResponse>,
    pub future_closures: Vec<ClosureResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct OrgUnitType {
    pub id: i32,
    pub label: String,
    pub parent: Option<i32>,
    pub can_have_staff: bool,
    pub can_have_patrons: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct OrgUnit {
    pub id: i32,
    pub uuid: String,
    pub code: String,
    pub label: String,
    pub parent: Option<i32>,
    pub unit_type: OrgUnitType,
    pub timezone: Option<String>,
    #[serde(skip_serializing)]
    pub children: Vec<OrgUnit>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrgUnitResponse {
    id: i32,
    /// Stable, DB-independent identity (see odo-durable-references).
    uuid: String,
    code: String,
    label: String,
    parent: Option<i32>,
    timezone: Option<String>,
    unit_type: OrgUnitType,
    /// RFC3339 soft-delete timestamp; null for active units. Only ever set on
    /// resolve-by-id responses (GET unit/{id}) — tree/scoping stay active-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    deleted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(no_recursion)]
    children: Option<Vec<OrgUnitResponse>>,
}

impl From<&OrgUnit> for OrgUnitResponse {
    fn from(u: &OrgUnit) -> Self {
        Self {
            id: u.id,
            uuid: u.uuid.clone(),
            code: u.code.clone(),
            label: u.label.clone(),
            parent: u.parent,
            timezone: u.timezone.clone(),
            unit_type: u.unit_type.clone(),
            deleted_at: None,
            children: None,
        }
    }
}

fn tree_response(u: &OrgUnit) -> OrgUnitResponse {
    OrgUnitResponse {
        id: u.id,
        uuid: u.uuid.clone(),
        code: u.code.clone(),
        label: u.label.clone(),
        parent: u.parent,
        timezone: u.timezone.clone(),
        unit_type: u.unit_type.clone(),
        deleted_at: None,
        children: Some(u.children.iter().map(tree_response).collect()),
    }
}

use odo_client::error::ApiError;

#[utoipa::path(
    get,
    path = "/api/v1/odo/org/tree",
    responses((status = 200, description = "Full org unit tree", body = OrgUnitResponse)),
    security(("bearer" = [])),
    tag = "org"
)]
pub async fn org_unit_tree(State(state): State<Arc<AppState>>) -> ApiResult<Json<OrgUnitResponse>> {
    let (tree, _) = load_org_tree(&state.db).await?;
    Ok(Json(tree_response(&tree)))
}

#[utoipa::path(
    get,
    path = "/api/v1/odo/org/root",
    responses((status = 200, body = OrgUnitResponse, description = "The root org unit (flat, no children)")),
    security(("bearer" = [])),
    tag = "org"
)]
pub async fn org_unit_root(State(state): State<Arc<AppState>>) -> ApiResult<Json<OrgUnitResponse>> {
    let (tree, _) = load_org_tree(&state.db).await?;
    Ok(Json(OrgUnitResponse::from(&tree)))
}

#[utoipa::path(
    get,
    path = "/api/v1/odo/org/unit/{id}",
    params(("id" = i32, Path, description = "Org unit ID")),
    responses((status = 200, body = OrgUnitDetailResponse, description = "Org unit detail with addresses, hours, and closures")),
    security(("bearer" = [])),
    tag = "org"
)]
pub async fn org_unit_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> ApiResult<Json<OrgUnitDetailResponse>> {
    let (_, org_map) = load_org_tree(&state.db).await?;

    // Active units come from the in-memory map; on a miss, resolve-by-id falls
    // back to the DB so soft-deleted units still resolve (flagged), keeping
    // historical references renderable. Unknown ids 404 as before.
    let org_unit = match org_map.get(&id) {
        Some(u) => OrgUnitResponse::from(u),
        None => {
            let (u, ut) = unit::Entity::find_by_id(id)
                .find_also_related(unit_type::Entity)
                .one(&state.db)
                .await?
                .ok_or(LocalError::not_found("org unit"))?;
            let ut = ut.ok_or(LocalError::internal("org unit has no unit_type"))?;
            OrgUnitResponse {
                id: u.id,
                uuid: u.uuid.to_string(),
                code: u.code,
                label: u.label,
                parent: u.parent,
                timezone: u.timezone,
                unit_type: OrgUnitType {
                    id: ut.id,
                    label: ut.label,
                    parent: ut.parent,
                    can_have_staff: ut.can_have_staff,
                    can_have_patrons: ut.can_have_patrons,
                },
                deleted_at: u.deleted_at.map(|d| d.to_rfc3339()),
                children: None,
            }
        }
    };

    let addresses: Vec<AddressResponse> = address::Entity::find()
        .filter(address::Column::OrgUnit.eq(id))
        .filter(address::Column::DeletedAt.is_null())
        .order_by_asc(address::Column::AddressType)
        .order_by_asc(address::Column::Id)
        .all(&state.db)
        .await?
        .into_iter()
        .map(AddressResponse::from)
        .collect();

    let operating_hours: Vec<OperatingHoursResponse> = operating_hours::Entity::find()
        .filter(operating_hours::Column::OrgUnit.eq(id))
        .order_by_asc(operating_hours::Column::DayOfWeek)
        .order_by_asc(operating_hours::Column::OpenTime)
        .all(&state.db)
        .await?
        .into_iter()
        .map(OperatingHoursResponse::from)
        .collect();

    let today = Utc::now().date_naive();
    let future_closures: Vec<ClosureResponse> = closure::Entity::find()
        .filter(closure::Column::OrgUnit.eq(id))
        .filter(closure::Column::ClosureDate.gte(today))
        .order_by_asc(closure::Column::ClosureDate)
        .order_by_asc(closure::Column::Id)
        .all(&state.db)
        .await?
        .into_iter()
        .map(ClosureResponse::from)
        .collect();

    Ok(Json(OrgUnitDetailResponse {
        org_unit,
        addresses,
        operating_hours,
        future_closures,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LabelBatchRequest {
    /// Lookup by database id.
    #[serde(default)]
    pub ids: Vec<i32>,
    /// Lookup by stable uuid (durable references). May be mixed with `ids`;
    /// entries are deduplicated in the response.
    #[serde(default)]
    pub uuids: Vec<String>,
    /// Opt in to also resolve soft-deleted units (flagged with deleted_at).
    /// Default false keeps this active-only, so write-time existence checks
    /// still reject deleted ids.
    #[serde(default)]
    pub include_deleted: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrgUnitLabelEntry {
    pub id: i32,
    pub uuid: String,
    pub code: String,
    pub label: String,
    /// RFC3339 soft-delete timestamp; null for active units.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LabelBatchResponse {
    pub labels: Vec<OrgUnitLabelEntry>,
}

/// Batch lookup of org-unit `(code, label)` keyed by id.
///
/// Built for callers that hold a set of org_unit ids and just need
/// display labels — e.g. patron.search's per-page decoration. Unknown ids
/// are silently dropped from the response.
#[utoipa::path(
    post,
    path = "/api/v1/odo/org/unit/label-batch",
    request_body = LabelBatchRequest,
    responses((status = 200, body = LabelBatchResponse, description = "Labels for the requested org-unit ids")),
    security(("bearer" = [])),
    tag = "org"
)]
pub async fn org_unit_label_batch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LabelBatchRequest>,
) -> ApiResult<Json<LabelBatchResponse>> {
    let (_, org_map) = load_org_tree(&state.db).await?;

    // Collect keyed by id so mixed id/uuid requests dedupe naturally.
    let mut entries: std::collections::BTreeMap<i32, OrgUnitLabelEntry> =
        std::collections::BTreeMap::new();
    let mut missing_ids: Vec<i32> = Vec::new();
    let mut missing_uuids: Vec<Uuid> = Vec::new();

    for id in req.ids {
        if let Some(unit) = org_map.get(&id) {
            entries.insert(
                unit.id,
                OrgUnitLabelEntry {
                    id: unit.id,
                    uuid: unit.uuid.clone(),
                    code: unit.code.clone(),
                    label: unit.label.clone(),
                    deleted_at: None,
                },
            );
        } else {
            missing_ids.push(id);
        }
    }

    // Uuid lookups walk the active map first; parse failures are treated as
    // unknown (dropped), matching the unknown-id behavior.
    if !req.uuids.is_empty() {
        let by_uuid: std::collections::HashMap<&str, &OrgUnit> =
            org_map.values().map(|u| (u.uuid.as_str(), u)).collect();
        for raw in &req.uuids {
            if let Some(unit) = by_uuid.get(raw.as_str()) {
                entries.insert(
                    unit.id,
                    OrgUnitLabelEntry {
                        id: unit.id,
                        uuid: unit.uuid.clone(),
                        code: unit.code.clone(),
                        label: unit.label.clone(),
                        deleted_at: None,
                    },
                );
            } else if let Ok(u) = raw.parse::<Uuid>() {
                missing_uuids.push(u);
            }
        }
    }

    // Ids/uuids not in the active map are soft-deleted or unknown. When the
    // caller opts in, resolve soft-deleted units too (flagged) so historical
    // references render; truly-unknown refs stay dropped.
    if req.include_deleted && (!missing_ids.is_empty() || !missing_uuids.is_empty()) {
        let mut cond = sea_orm::Condition::any();
        if !missing_ids.is_empty() {
            cond = cond.add(unit::Column::Id.is_in(missing_ids));
        }
        if !missing_uuids.is_empty() {
            cond = cond.add(unit::Column::Uuid.is_in(missing_uuids));
        }
        let rows = unit::Entity::find().filter(cond).all(&state.db).await?;
        for u in rows {
            entries.insert(
                u.id,
                OrgUnitLabelEntry {
                    id: u.id,
                    uuid: u.uuid.to_string(),
                    code: u.code,
                    label: u.label,
                    deleted_at: u.deleted_at.map(|d| d.to_rfc3339()),
                },
            );
        }
    }

    Ok(Json(LabelBatchResponse {
        labels: entries.into_values().collect(),
    }))
}


/// Resolve a unit's database id from its stable uuid: active map first,
/// then the DB (so soft-deleted units resolve too, matching the id-based
/// detail fallback). 404 when unknown.
async fn unit_id_by_uuid(
    state: &AppState,
    raw: &str,
) -> Result<i32, ApiError> {
    let uuid: Uuid = raw
        .parse()
        .map_err(|_| LocalError::invalid_input("invalid uuid"))?;
    let (_, org_map) = load_org_tree(&state.db).await?;
    if let Some(u) = org_map.values().find(|u| u.uuid == raw) {
        return Ok(u.id);
    }
    let row = unit::Entity::find()
        .filter(unit::Column::Uuid.eq(uuid))
        .one(&state.db)
        .await?
        .ok_or(LocalError::not_found("org unit"))?;
    Ok(row.id)
}

/// Unit detail by stable uuid (durable references): same response and
/// soft-delete semantics as the id route.
#[utoipa::path(
    get,
    path = "/api/v1/odo/org/unit/uuid/{uuid}",
    params(("uuid" = String, Path, description = "Org unit uuid")),
    responses((status = 200, body = OrgUnitDetailResponse)),
    security(("bearer" = [])),
    tag = "org"
)]
pub async fn org_unit_detail_by_uuid(
    State(state): State<Arc<AppState>>,
    Path(raw): Path<String>,
) -> ApiResult<Json<OrgUnitDetailResponse>> {
    let id = unit_id_by_uuid(&state, &raw).await?;
    org_unit_detail(State(state), Path(id)).await
}

/// Ancestor chain by stable uuid.
#[utoipa::path(
    get,
    path = "/api/v1/odo/org/unit/uuid/{uuid}/ancestors",
    params(("uuid" = String, Path, description = "Org unit uuid")),
    responses((status = 200, body = Vec<OrgUnitResponse>)),
    security(("bearer" = [])),
    tag = "org"
)]
pub async fn org_unit_ancestors_by_uuid(
    State(state): State<Arc<AppState>>,
    Path(raw): Path<String>,
) -> ApiResult<Json<Vec<OrgUnitResponse>>> {
    let id = unit_id_by_uuid(&state, &raw).await?;
    org_unit_ancestors(State(state), Path(id)).await
}

/// Descendant subtree by stable uuid.
#[utoipa::path(
    get,
    path = "/api/v1/odo/org/unit/uuid/{uuid}/descendants",
    params(("uuid" = String, Path, description = "Org unit uuid")),
    responses((status = 200, body = Vec<OrgUnitResponse>)),
    security(("bearer" = [])),
    tag = "org"
)]
pub async fn org_unit_descendants_by_uuid(
    State(state): State<Arc<AppState>>,
    Path(raw): Path<String>,
) -> ApiResult<Json<Vec<OrgUnitResponse>>> {
    let id = unit_id_by_uuid(&state, &raw).await?;
    org_unit_descendants(State(state), Path(id)).await
}

#[utoipa::path(
    get,
    path = "/api/v1/odo/org/unit/{id}/ancestors",
    params(("id" = i32, Path, description = "Org unit ID")),
    responses((status = 200, description = "Ancestor chain from root to target", body = Vec<OrgUnitResponse>)),
    security(("bearer" = [])),
    tag = "org"
)]
pub async fn org_unit_ancestors(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> ApiResult<Json<Vec<OrgUnitResponse>>> {
    let (_, org_map) = load_org_tree(&state.db).await?;

    let mut lineage = Vec::new();
    let mut current_id = id;

    loop {
        let node = org_map
            .get(&current_id)
            .ok_or(LocalError::not_found("org unit"))?;

        lineage.push(OrgUnitResponse::from(node));

        if let Some(parent_id) = node.parent {
            current_id = parent_id;
        } else {
            break;
        }
    }

    lineage.reverse();
    Ok(Json(lineage))
}

#[utoipa::path(
    get,
    path = "/api/v1/odo/org/unit/{id}/descendants",
    params(("id" = i32, Path, description = "Org unit ID")),
    responses((status = 200, description = "All descendants (flat list)", body = Vec<OrgUnitResponse>)),
    security(("bearer" = [])),
    tag = "org"
)]
pub async fn org_unit_descendants(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> ApiResult<Json<Vec<OrgUnitResponse>>> {
    let (_, org_map) = load_org_tree(&state.db).await?;

    let mut queue = VecDeque::new();
    queue.push_back(id);
    let mut descendants = Vec::new();

    while let Some(current_id) = queue.pop_front() {
        let node = org_map
            .get(&current_id)
            .ok_or(LocalError::not_found("org unit"))?;

        descendants.push(OrgUnitResponse::from(node));

        for child in &node.children {
            queue.push_back(child.id);
        }
    }

    Ok(Json(descendants))
}

/// Build the org tree and an id→unit map fresh from the database.
///
/// The org tree changes rarely but reads must always reflect the latest
/// edits (there are multiple odo-org replicas, and org admin writes happen
/// in this same service), so we query per request rather than cache. It is
/// one indexed query plus an in-memory tree build.
async fn load_org_tree(
    db: &DatabaseConnection,
) -> Result<(OrgUnit, HashMap<i32, OrgUnit>), ApiError> {
    let units: Vec<(unit::Model, Option<unit_type::Model>)> = unit::Entity::find()
        .filter(unit::Column::DeletedAt.is_null())
        .find_also_related(unit_type::Entity)
        .all(db)
        .await?;

    let mut units_by_id: HashMap<i32, OrgUnit> = HashMap::new();
    let mut parent_to_children: HashMap<i32, Vec<i32>> = HashMap::new();
    let mut root_ids: Vec<i32> = Vec::new();

    for (u, ut) in units {
        let ut = ut.ok_or(LocalError::internal(format!(
            "Org unit {} has no unit_type",
            u.id
        )))?;

        if ut.deleted_at.is_some() {
            continue;
        }

        if let Some(parent_id) = u.parent {
            parent_to_children.entry(parent_id).or_default().push(u.id);
        } else {
            root_ids.push(u.id);
        }

        units_by_id.insert(
            u.id,
            OrgUnit {
                id: u.id,
                uuid: u.uuid.to_string(),
                code: u.code,
                label: u.label,
                parent: u.parent,
                timezone: u.timezone,
                unit_type: OrgUnitType {
                    id: ut.id,
                    label: ut.label,
                    parent: ut.parent,
                    can_have_staff: ut.can_have_staff,
                    can_have_patrons: ut.can_have_patrons,
                },
                children: Vec::new(),
            },
        );
    }

    let root_id = match root_ids.len() {
        0 => return Err(LocalError::internal("No root org unit found").into()),
        1 => root_ids[0],
        _ => {
            return Err(
                LocalError::internal(format!("Multiple root org units: {:?}", root_ids)).into(),
            );
        }
    };

    let tree = build_org_tree(root_id, &units_by_id, &parent_to_children)?;

    let mut org_map = HashMap::new();
    populate_org_map(&tree, &mut org_map);

    Ok((tree, org_map))
}

fn build_org_tree(
    root_id: i32,
    units: &HashMap<i32, OrgUnit>,
    child_lookup: &HashMap<i32, Vec<i32>>,
) -> Result<OrgUnit, ApiError> {
    fn build_node(
        node_id: i32,
        units: &HashMap<i32, OrgUnit>,
        child_lookup: &HashMap<i32, Vec<i32>>,
    ) -> Result<OrgUnit, ApiError> {
        let mut node = units
            .get(&node_id)
            .cloned()
            .ok_or(LocalError::internal(format!(
                "Missing org unit {} referenced as parent",
                node_id
            )))?;

        if let Some(children) = child_lookup.get(&node_id) {
            let mut built = Vec::with_capacity(children.len());
            for child_id in children {
                built.push(build_node(*child_id, units, child_lookup)?);
            }
            node.children = built;
        }

        Ok(node)
    }

    build_node(root_id, units, child_lookup)
}

fn populate_org_map(node: &OrgUnit, map: &mut HashMap<i32, OrgUnit>) {
    map.insert(node.id, node.clone());
    for child in &node.children {
        populate_org_map(child, map);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_unit_type() -> OrgUnitType {
        OrgUnitType {
            id: 1,
            label: "Branch".to_string(),
            parent: None,
            can_have_staff: true,
            can_have_patrons: true,
        }
    }

    fn make_unit(id: i32, parent: Option<i32>, label: &str) -> OrgUnit {
        OrgUnit {
            id,
            code: format!("U{id}"),
            uuid: format!("UUID-{id}"),
            label: label.to_string(),
            parent,
            timezone: None,
            unit_type: make_unit_type(),
            children: Vec::new(),
        }
    }

    fn sample_units() -> (HashMap<i32, OrgUnit>, HashMap<i32, Vec<i32>>) {
        //   1 (root)
        //  / \
        // 2   3
        //     |
        //     4
        let mut units = HashMap::new();
        units.insert(1, make_unit(1, None, "Root"));
        units.insert(2, make_unit(2, Some(1), "Branch A"));
        units.insert(3, make_unit(3, Some(1), "Branch B"));
        units.insert(4, make_unit(4, Some(3), "Sub-Branch"));

        let mut children = HashMap::new();
        children.insert(1, vec![2, 3]);
        children.insert(3, vec![4]);

        (units, children)
    }

    #[test]
    fn build_tree_structure() {
        let (units, children) = sample_units();
        let tree = build_org_tree(1, &units, &children).unwrap();

        assert_eq!(tree.id, 1);
        assert_eq!(tree.children.len(), 2);

        let branch_b = tree.children.iter().find(|c| c.id == 3).unwrap();
        assert_eq!(branch_b.children.len(), 1);
        assert_eq!(branch_b.children[0].id, 4);
    }

    #[test]
    fn build_tree_leaf_has_no_children() {
        let (units, children) = sample_units();
        let tree = build_org_tree(1, &units, &children).unwrap();

        let branch_a = tree.children.iter().find(|c| c.id == 2).unwrap();
        assert!(branch_a.children.is_empty());

        let sub = tree
            .children
            .iter()
            .find(|c| c.id == 3)
            .unwrap()
            .children
            .iter()
            .find(|c| c.id == 4)
            .unwrap();
        assert!(sub.children.is_empty());
    }

    #[test]
    fn build_tree_missing_root_errors() {
        let (units, children) = sample_units();
        let result = build_org_tree(999, &units, &children);
        assert!(result.is_err());
    }

    #[test]
    fn populate_map_flattens_tree() {
        let (units, children) = sample_units();
        let tree = build_org_tree(1, &units, &children).unwrap();

        let mut map = HashMap::new();
        populate_org_map(&tree, &mut map);

        assert_eq!(map.len(), 4);
        assert!(map.contains_key(&1));
        assert!(map.contains_key(&2));
        assert!(map.contains_key(&3));
        assert!(map.contains_key(&4));
    }

    #[test]
    fn populate_map_preserves_parent() {
        let (units, children) = sample_units();
        let tree = build_org_tree(1, &units, &children).unwrap();

        let mut map = HashMap::new();
        populate_org_map(&tree, &mut map);

        assert_eq!(map[&1].parent, None);
        assert_eq!(map[&2].parent, Some(1));
        assert_eq!(map[&4].parent, Some(3));
    }

    #[test]
    fn tree_response_includes_children_recursively() {
        let (units, children) = sample_units();
        let tree = build_org_tree(1, &units, &children).unwrap();

        let resp = tree_response(&tree);
        assert!(resp.children.is_some());
        let root_children = resp.children.as_ref().unwrap();
        assert_eq!(root_children.len(), 2);

        let branch_b = root_children.iter().find(|c| c.id == 3).unwrap();
        assert!(branch_b.children.is_some());
        assert_eq!(branch_b.children.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn org_unit_response_from_omits_children() {
        let unit = make_unit(5, Some(1), "Test");
        let resp = OrgUnitResponse::from(&unit);
        assert!(resp.children.is_none());
        assert_eq!(resp.id, 5);
        assert_eq!(resp.parent, Some(1));
    }
}
