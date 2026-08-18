-- Tests for authz.usr_perm_scopes() and authz.usr_covered_units()
--
-- usr_perm_scopes(usr) returns the collapsed, per-permission effective scope:
--   * is_global rows (covers the root) => permission applies everywhere.
--   * otherwise one row per *minimal* covered subtree root ("<label> and below").
-- We assert both the covered-unit membership (matches usr_has_perm_at) and the
-- collapsed display roots.

BEGIN;

SELECT plan(31);

-- Shared org hierarchy (see seed/org_unit_data.sql):
--   9000 Root
--   9001 Region North          9002 Region South
--   9010 Branch N-A  9011 N-B   9012 Branch S-A  9013 S-B
--   9020 Dept N-A1  9021 N-B1   9022 Dept S-A1
\i seed/org_unit_data.sql

INSERT INTO auth.usr (id, auth_method, username, email) VALUES
    (9000, 'local', 'test.user', 'test.user@localhost');

INSERT INTO authz.role (code, label, description) VALUES
    ('scope_role', 'Scope Role', 'Role for scope tests'),
    ('scope_role_b', 'Scope Role B', 'Second role for union tests');

INSERT INTO authz.permission (code, description) VALUES
    ('scope.global', 'min_depth 0 permission'),
    ('scope.regional', 'min_depth 1 permission'),
    ('scope.branch', 'min_depth 2 permission');

-- Helper assertions are expressed with plain SQL against the functions.

-- =============================================================================
-- SUITE 1: Global permission (root covered) -> is_global, no scope units
-- =============================================================================
-- Role at root, min_depth 0.
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('scope_role', 'scope.global', 0);
INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9000, 'scope_role', 9000);

SELECT is(
    (SELECT count(*)::int FROM authz.usr_covered_units(9000, 'scope.global')),
    10,
    'global: covers all 10 org units'
);

SELECT ok(
    (SELECT is_global FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.global'),
    'global: is_global is true'
);

SELECT is(
    (SELECT count(*)::int FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.global'),
    1,
    'global: exactly one summary row'
);

SELECT is(
    (SELECT scope_unit_id FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.global'),
    NULL,
    'global: scope_unit_id is NULL'
);

DELETE FROM authz.usr_role_org_map WHERE usr = 9000;
DELETE FROM authz.role_permission WHERE role = 'scope_role';

-- =============================================================================
-- SUITE 2: Regional scope (min_depth 1, assignment at a branch)
--   Expands up to Region North, covering it and all its branches/depts.
--   Minimal root: just Region North (9001).
-- =============================================================================
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('scope_role', 'scope.regional', 1);
INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9000, 'scope_role', 9010); -- Branch North-A (depth 2)

SELECT ok(
    NOT (SELECT is_global FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.regional'),
    'regional: not global'
);

SELECT is(
    (SELECT count(*)::int FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.regional'),
    1,
    'regional: a single minimal scope root'
);

SELECT is(
    (SELECT scope_unit_id FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.regional'),
    9001,
    'regional: the scope root is Region North'
);

SELECT is(
    (SELECT scope_unit_label FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.regional'),
    'Test Region North',
    'regional: scope label resolved'
);

-- Covered set matches enforcement: Region North + its two branches + two depts,
-- NOT root, NOT South. (9001, 9010, 9011, 9020, 9021)
SELECT is(
    (SELECT count(*)::int FROM authz.usr_covered_units(9000, 'scope.regional')),
    5,
    'regional: covers region + 2 branches + 2 depts'
);
SELECT ok(
    NOT EXISTS (SELECT 1 FROM authz.usr_covered_units(9000, 'scope.regional') WHERE unit_id = 9000),
    'regional: does not cover root'
);
SELECT ok(
    NOT EXISTS (SELECT 1 FROM authz.usr_covered_units(9000, 'scope.regional') WHERE unit_id = 9002),
    'regional: does not cover Region South'
);
SELECT ok(
    EXISTS (SELECT 1 FROM authz.usr_covered_units(9000, 'scope.regional') WHERE unit_id = 9011),
    'regional: covers sibling Branch North-B'
);

DELETE FROM authz.usr_role_org_map WHERE usr = 9000;
DELETE FROM authz.role_permission WHERE role = 'scope_role';

-- =============================================================================
-- SUITE 3: Branch scope (min_depth 2, assignment at a branch) -> no sideways
--   Minimal root: the branch itself (9010). Covers branch + its dept only.
-- =============================================================================
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('scope_role', 'scope.branch', 2);
INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9000, 'scope_role', 9010); -- Branch North-A

SELECT is(
    (SELECT scope_unit_id FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.branch'),
    9010,
    'branch: scope root is the branch itself'
);
SELECT is(
    (SELECT count(*)::int FROM authz.usr_covered_units(9000, 'scope.branch')),
    2,
    'branch: covers branch + its dept only'
);
SELECT ok(
    NOT EXISTS (SELECT 1 FROM authz.usr_covered_units(9000, 'scope.branch') WHERE unit_id = 9011),
    'branch: no sibling expansion'
);

DELETE FROM authz.usr_role_org_map WHERE usr = 9000;
DELETE FROM authz.role_permission WHERE role = 'scope_role';

-- =============================================================================
-- SUITE 4: min_depth > assignment depth -> descendants at/below min_depth,
--   producing MULTIPLE minimal roots (one per qualifying descendant).
--   Assignment at Region North (depth 1), min_depth 2 -> covers the region's
--   branches (depth 2) and below, but NOT the region itself.
--   Minimal roots: Branch N-A (9010) and Branch N-B (9011).
-- =============================================================================
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('scope_role', 'scope.branch', 2);
INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9000, 'scope_role', 9001); -- Region North (depth 1)

SELECT ok(
    NOT (SELECT bool_or(is_global) FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.branch'),
    'depth>assignment: not global'
);
SELECT is(
    (SELECT count(*)::int FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.branch'),
    2,
    'depth>assignment: two minimal roots (the two branches)'
);
SELECT results_eq(
    $$ SELECT scope_unit_id FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.branch' ORDER BY scope_unit_id $$,
    $$ VALUES (9010), (9011) $$,
    'depth>assignment: roots are Branch N-A and N-B'
);
SELECT ok(
    NOT EXISTS (SELECT 1 FROM authz.usr_covered_units(9000, 'scope.branch') WHERE unit_id = 9001),
    'depth>assignment: region itself not covered'
);

DELETE FROM authz.usr_role_org_map WHERE usr = 9000;
DELETE FROM authz.role_permission WHERE role = 'scope_role';

-- =============================================================================
-- SUITE 5: Redundant-subtree collapse.
--   Two assignments for the same permission: one at Region North (min_depth 1
--   -> the whole region) and one at Branch N-A (also min_depth 1 -> same region).
--   The union is just Region North; the display must collapse to ONE root, not
--   list Branch N-A separately (it is a descendant of the region root).
-- =============================================================================
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('scope_role', 'scope.regional', 1);
INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9000, 'scope_role', 9001),  -- Region North (min_depth 1 -> Region North)
    (9000, 'scope_role', 9010);  -- Branch N-A   (min_depth 1 -> Region North too)

SELECT is(
    (SELECT count(*)::int FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.regional'),
    1,
    'collapse: overlapping assignments collapse to one root'
);
SELECT is(
    (SELECT scope_unit_id FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.regional'),
    9001,
    'collapse: the single root is Region North'
);

DELETE FROM authz.usr_role_org_map WHERE usr = 9000;
DELETE FROM authz.role_permission WHERE role = 'scope_role';

-- =============================================================================
-- SUITE 6: Two disjoint regional roots (different regions).
--   Assignments at Branch N-A and Branch S-A, min_depth 1 -> Region North and
--   Region South. Two minimal roots.
-- =============================================================================
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('scope_role', 'scope.regional', 1);
INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9000, 'scope_role', 9010),  -- Branch N-A -> Region North
    (9000, 'scope_role', 9012);  -- Branch S-A -> Region South

SELECT is(
    (SELECT count(*)::int FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.regional'),
    2,
    'two regions: two minimal roots'
);
SELECT results_eq(
    $$ SELECT scope_unit_id FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.regional' ORDER BY scope_unit_id $$,
    $$ VALUES (9001), (9002) $$,
    'two regions: roots are Region North and Region South'
);

DELETE FROM authz.usr_role_org_map WHERE usr = 9000;
DELETE FROM authz.role_permission WHERE role = 'scope_role';

-- =============================================================================
-- SUITE 7: Union across permissions + global wins.
--   scope.global via a root min_depth-0 role AND scope.regional at a branch.
--   scope.global -> is_global; scope.regional -> Region North.
-- =============================================================================
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('scope_role', 'scope.global', 0),
    ('scope_role_b', 'scope.regional', 1);
INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9000, 'scope_role', 9000),   -- global at root
    (9000, 'scope_role_b', 9010); -- regional at Branch N-A

SELECT is(
    (SELECT count(DISTINCT perm)::int FROM authz.usr_perm_scopes(9000)),
    2,
    'union: two distinct permissions reported'
);
SELECT ok(
    (SELECT is_global FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.global'),
    'union: scope.global is global'
);
SELECT is(
    (SELECT scope_unit_id FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.regional'),
    9001,
    'union: scope.regional root is Region North'
);

DELETE FROM authz.usr_role_org_map WHERE usr = 9000;
DELETE FROM authz.role_permission WHERE role IN ('scope_role', 'scope_role_b');

-- =============================================================================
-- SUITE 8: Same permission granted by two roles at different min_depths ->
--   the broader (shallower) scope should win via the covered-set union.
--   scope_role grants scope.regional min_depth 1; scope_role_b grants
--   scope.regional min_depth 2. Assignments: scope_role at Branch N-A (=> whole
--   Region North) and scope_role_b at Branch S-A (=> Branch S-A only).
--   Roots: Region North (9001) and Branch S-A (9012).
-- =============================================================================
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('scope_role', 'scope.regional', 1),
    ('scope_role_b', 'scope.regional', 2);
INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9000, 'scope_role', 9010),    -- min_depth 1 -> Region North
    (9000, 'scope_role_b', 9012);  -- min_depth 2 -> Branch S-A only

SELECT is(
    (SELECT count(*)::int FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.regional'),
    2,
    'mixed min_depth: two roots'
);
SELECT results_eq(
    $$ SELECT scope_unit_id FROM authz.usr_perm_scopes(9000) WHERE perm = 'scope.regional' ORDER BY scope_unit_id $$,
    $$ VALUES (9001), (9012) $$,
    'mixed min_depth: Region North (broad) + Branch S-A (narrow)'
);
SELECT ok(
    NOT EXISTS (SELECT 1 FROM authz.usr_covered_units(9000, 'scope.regional') WHERE unit_id = 9013),
    'mixed min_depth: Branch S-B (sibling of S-A) not covered'
);

DELETE FROM authz.usr_role_org_map WHERE usr = 9000;
DELETE FROM authz.role_permission WHERE role IN ('scope_role', 'scope_role_b');

-- =============================================================================
-- SUITE 9: A user with no assignments reports no scope rows.
-- =============================================================================
SELECT is(
    (SELECT count(*)::int FROM authz.usr_perm_scopes(9000)),
    0,
    'no assignments: no scope rows'
);

-- Parity spot-check: every covered unit for scope.regional (SUITE 2 config) is
-- exactly where usr_has_perm_at agrees. Re-establish that config briefly.
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('scope_role', 'scope.regional', 1);
INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9000, 'scope_role', 9010);

SELECT ok(
    NOT EXISTS (
        SELECT u.id
        FROM org.unit u
        WHERE (u.id IN (SELECT unit_id FROM authz.usr_covered_units(9000, 'scope.regional')))
              <> authz.usr_has_perm_at(9000, 'scope.regional', u.id)
    ),
    'parity: usr_covered_units matches usr_has_perm_at at every unit'
);

DELETE FROM authz.usr_role_org_map WHERE usr = 9000;
DELETE FROM authz.role_permission WHERE role = 'scope_role';

SELECT * FROM finish();

ROLLBACK;
