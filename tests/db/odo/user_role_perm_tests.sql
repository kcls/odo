-- Deploy odo-db-tests:001_role_permission_tests to pg
-- Tests for authz.usr_has_perm_at() with various depth configurations

BEGIN;

SELECT plan(55);

-- Shared org unit hierarchy (see seed/org_unit_data.sql for full tree)
\i seed/org_unit_data.sql

-- Create test user
INSERT INTO auth.usr (id, auth_method, username, email) VALUES
    (9000, 'local', 'test.user', 'test.user@localhost');

-- Create test role
INSERT INTO authz.role (code, label, description) VALUES
    ('test_role', 'Test Role', 'Test role for permission depth testing');

-- Create test permissions
INSERT INTO authz.permission (code, description) VALUES
    ('test.read', 'Test read permission'),
    ('test.write', 'Test write permission'),
    ('test.admin', 'Test admin permission');

-- =============================================================================
-- TEST SUITE 1: Role at Root (depth 0) with various permission min_depths
-- =============================================================================

-- Test 1.1: Role at root (depth 0), permission min_depth 0 -> grants everywhere
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('test_role', 'test.read', 0);

INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9000, 'test_role', 9000);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.read', 9000),
    'Role at root, min_depth 0: has permission at root'
);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.read', 9001),
    'Role at root, min_depth 0: has permission at region'
);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.read', 9010),
    'Role at root, min_depth 0: has permission at branch'
);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.read', 9020),
    'Role at root, min_depth 0: has permission at department'
);

-- Clean up for next test
DELETE FROM authz.usr_role_org_map WHERE usr = 9000;
DELETE FROM authz.role_permission WHERE role = 'test_role';

-- Test 1.2: Role at root (depth 0), permission min_depth 2 -> grants at depth >= 2 only
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('test_role', 'test.write', 2);

INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9000, 'test_role', 9000);

SELECT ok(
    NOT authz.usr_has_perm_at(9000, 'test.write', 9000),
    'Role at root, min_depth 2: NO permission at root (depth 0)'
);

SELECT ok(
    NOT authz.usr_has_perm_at(9000, 'test.write', 9001),
    'Role at root, min_depth 2: NO permission at region (depth 1)'
);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.write', 9010),
    'Role at root, min_depth 2: has permission at branch (depth 2)'
);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.write', 9020),
    'Role at root, min_depth 2: has permission at department (depth 3)'
);

-- Clean up for next test suite
DELETE FROM authz.usr_role_org_map WHERE usr = 9000;
DELETE FROM authz.role_permission WHERE role = 'test_role';

-- =============================================================================
-- TEST SUITE 2: Role at Branch (depth 2) with various permission min_depths
-- =============================================================================

-- Test 2.1: Role at branch (depth 2), permission min_depth 0 -> grants at all ancestors and descendants
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('test_role', 'test.admin', 0);

INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9000, 'test_role', 9010);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.admin', 9000),
    'Role at branch, min_depth 0: has permission at root'
);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.admin', 9001),
    'Role at branch, min_depth 0: has permission at region'
);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.admin', 9010),
    'Role at branch, min_depth 0: has permission at branch (role org)'
);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.admin', 9020),
    'Role at branch, min_depth 0: has permission at department'
);

-- Clean up for next test
DELETE FROM authz.usr_role_org_map WHERE usr = 9000;
DELETE FROM authz.role_permission WHERE role = 'test_role';

-- Test 2.2: Role at branch (depth 2), permission min_depth 1 -> grants at region and below
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('test_role', 'test.read', 1);

INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9000, 'test_role', 9010);

SELECT ok(
    NOT authz.usr_has_perm_at(9000, 'test.read', 9000),
    'Role at branch, min_depth 1: NO permission at root (depth 0)'
);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.read', 9001),
    'Role at branch, min_depth 1: has permission at region (depth 1)'
);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.read', 9010),
    'Role at branch, min_depth 1: has permission at branch (depth 2)'
);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.read', 9020),
    'Role at branch, min_depth 1: has permission at department (depth 3)'
);

-- Clean up for next test
DELETE FROM authz.usr_role_org_map WHERE usr = 9000;
DELETE FROM authz.role_permission WHERE role = 'test_role';

-- Test 2.3: Role at branch (depth 2), permission min_depth 2 -> grants at branch and below only
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('test_role', 'test.write', 2);

INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9000, 'test_role', 9010);

SELECT ok(
    NOT authz.usr_has_perm_at(9000, 'test.write', 9000),
    'Role at branch, min_depth 2: NO permission at root'
);

SELECT ok(
    NOT authz.usr_has_perm_at(9000, 'test.write', 9001),
    'Role at branch, min_depth 2: NO permission at region'
);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.write', 9010),
    'Role at branch, min_depth 2: has permission at branch (depth 2)'
);

SELECT ok(
    authz.usr_has_perm_at(9000, 'test.write', 9020),
    'Role at branch, min_depth 2: has permission at department (depth 3)'
);

-- Clean up for next test suite
DELETE FROM authz.usr_role_org_map WHERE usr = 9000;
DELETE FROM authz.role_permission WHERE role = 'test_role';

-- =============================================================================
-- TEST SUITE 3: Sibling expansion with min_depth
-- =============================================================================

-- Create additional test user
INSERT INTO auth.usr (id, auth_method, username, email) VALUES
    (9100, 'local', 'test.sibling', 'test.sibling@localhost');

-- Create test role for sibling tests
INSERT INTO authz.role (code, label, description) VALUES
    ('test_manager', 'Test Manager', 'Test role for sibling expansion');

-- Create test permissions for sibling tests
INSERT INTO authz.permission (code, description) VALUES
    ('test.global', 'Global permission with min_depth 0'),
    ('test.regional', 'Regional permission with min_depth 1'),
    ('test.branch', 'Branch permission with min_depth 2');

-- Test 3.1: Role at Branch North-A with min_depth 0 - has permission everywhere
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('test_manager', 'test.global', 0);

INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9100, 'test_manager', 9010); -- Branch North-A at depth 2

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.global', 9000),
    'Role at Branch North-A, min_depth 0: has permission at root'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.global', 9001),
    'Role at Branch North-A, min_depth 0: has permission at Region North'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.global', 9002),
    'Role at Branch North-A, min_depth 0: has permission at Region South'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.global', 9011),
    'Role at Branch North-A, min_depth 0: has permission at sibling Branch North-B'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.global', 9012),
    'Role at Branch North-A, min_depth 0: has permission at Branch South-A (different region)'
);

-- Clean up
DELETE FROM authz.usr_role_org_map WHERE usr = 9100;
DELETE FROM authz.role_permission WHERE role = 'test_manager';

-- Test 3.2: Role at Branch North-A with min_depth 1 - regional with sibling expansion
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('test_manager', 'test.regional', 1);

INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9100, 'test_manager', 9010); -- Branch North-A at depth 2

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.regional', 9000),
    'Role at Branch North-A, min_depth 1: NO permission at root (depth 0)'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.regional', 9001),
    'Role at Branch North-A, min_depth 1: has permission at Region North (depth 1)'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.regional', 9011),
    'Role at Branch North-A, min_depth 1: has permission at sibling Branch North-B'
);

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.regional', 9002),
    'Role at Branch North-A, min_depth 1: NO permission at Region South'
);

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.regional', 9012),
    'Role at Branch North-A, min_depth 1: NO permission at Branch South-A (different region)'
);

-- Clean up
DELETE FROM authz.usr_role_org_map WHERE usr = 9100;
DELETE FROM authz.role_permission WHERE role = 'test_manager';

-- Test 3.3: Role at Branch North-A with min_depth 2 - no sibling expansion
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('test_manager', 'test.branch', 2);

INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9100, 'test_manager', 9010); -- Branch North-A at depth 2

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.branch', 9010),
    'Role at Branch North-A, min_depth 2: has permission at Branch North-A (depth 2)'
);

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.branch', 9001),
    'Role at Branch North-A, min_depth 2: NO permission at Region North'
);

-- Clean up
DELETE FROM authz.usr_role_org_map WHERE usr = 9100;
DELETE FROM authz.role_permission WHERE role = 'test_manager';

-- =============================================================================
-- TEST SUITE 4: Multi-branch role assignments
-- =============================================================================
-- Uses the org structure from shared seed:
--   9000: Root (depth 0)
--   9001: Region North (depth 1)       9002: Region South (depth 1)
--   9010: Branch N-A (depth 2, North)  9012: Branch S-A (depth 2, South)
--   9011: Branch N-B (depth 2, North)
--
-- Reuses test user 9100 (test.sibling) and test_manager role.

-- -----------------------------------------------------------------------------
-- Test 4.1: Roles at two branches in the SAME region (N-A + N-B, both under North)
--           with min_depth 1 (regional)
-- Expected: permission at Region North and all its branches, NOT at South
-- -----------------------------------------------------------------------------
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('test_manager', 'test.regional', 1);

INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9100, 'test_manager', 9010), -- Branch North-A
    (9100, 'test_manager', 9011); -- Branch North-B (same region)

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.regional', 9000),
    'Multi-branch same region, min_depth 1: NO permission at root'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.regional', 9001),
    'Multi-branch same region, min_depth 1: has permission at Region North'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.regional', 9010),
    'Multi-branch same region, min_depth 1: has permission at Branch North-A'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.regional', 9011),
    'Multi-branch same region, min_depth 1: has permission at Branch North-B'
);

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.regional', 9002),
    'Multi-branch same region, min_depth 1: NO permission at Region South'
);

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.regional', 9012),
    'Multi-branch same region, min_depth 1: NO permission at Branch South-A'
);

-- Clean up
DELETE FROM authz.usr_role_org_map WHERE usr = 9100;
DELETE FROM authz.role_permission WHERE role = 'test_manager';

-- -----------------------------------------------------------------------------
-- Test 4.2: Roles at two branches in the SAME region (N-A + N-B)
--           with min_depth 2 (branch-scoped, no sibling expansion)
-- Expected: permission only at Branch N-A and Branch N-B, nowhere else
-- -----------------------------------------------------------------------------
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('test_manager', 'test.branch', 2);

INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9100, 'test_manager', 9010), -- Branch North-A
    (9100, 'test_manager', 9011); -- Branch North-B

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.branch', 9000),
    'Multi-branch same region, min_depth 2: NO permission at root'
);

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.branch', 9001),
    'Multi-branch same region, min_depth 2: NO permission at Region North'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.branch', 9010),
    'Multi-branch same region, min_depth 2: has permission at Branch North-A'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.branch', 9011),
    'Multi-branch same region, min_depth 2: has permission at Branch North-B'
);

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.branch', 9012),
    'Multi-branch same region, min_depth 2: NO permission at Branch South-A'
);

-- Clean up
DELETE FROM authz.usr_role_org_map WHERE usr = 9100;
DELETE FROM authz.role_permission WHERE role = 'test_manager';

-- -----------------------------------------------------------------------------
-- Test 4.3: Roles at two branches in DIFFERENT regions (N-A under North, S-A under South)
--           with min_depth 1 (regional)
-- Expected: permission at both regions and all their branches
-- -----------------------------------------------------------------------------
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('test_manager', 'test.regional', 1);

INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9100, 'test_manager', 9010), -- Branch North-A (North)
    (9100, 'test_manager', 9012); -- Branch South-A (South)

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.regional', 9000),
    'Multi-branch diff regions, min_depth 1: NO permission at root'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.regional', 9001),
    'Multi-branch diff regions, min_depth 1: has permission at Region North'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.regional', 9002),
    'Multi-branch diff regions, min_depth 1: has permission at Region South'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.regional', 9010),
    'Multi-branch diff regions, min_depth 1: has permission at Branch North-A'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.regional', 9011),
    'Multi-branch diff regions, min_depth 1: has permission at sibling Branch North-B'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.regional', 9012),
    'Multi-branch diff regions, min_depth 1: has permission at Branch South-A'
);

-- Clean up
DELETE FROM authz.usr_role_org_map WHERE usr = 9100;
DELETE FROM authz.role_permission WHERE role = 'test_manager';

-- -----------------------------------------------------------------------------
-- Test 4.4: Roles at two branches in DIFFERENT regions (N-A under North, S-A under South)
--           with min_depth 2 (branch-scoped, no sibling expansion)
-- Expected: permission only at Branch N-A and Branch S-A, not at siblings or regions
-- -----------------------------------------------------------------------------
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('test_manager', 'test.branch', 2);

INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (9100, 'test_manager', 9010), -- Branch North-A (North)
    (9100, 'test_manager', 9012); -- Branch South-A (South)

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.branch', 9000),
    'Multi-branch diff regions, min_depth 2: NO permission at root'
);

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.branch', 9001),
    'Multi-branch diff regions, min_depth 2: NO permission at Region North'
);

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.branch', 9002),
    'Multi-branch diff regions, min_depth 2: NO permission at Region South'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.branch', 9010),
    'Multi-branch diff regions, min_depth 2: has permission at Branch North-A'
);

SELECT ok(
    NOT authz.usr_has_perm_at(9100, 'test.branch', 9011),
    'Multi-branch diff regions, min_depth 2: NO permission at sibling Branch North-B'
);

SELECT ok(
    authz.usr_has_perm_at(9100, 'test.branch', 9012),
    'Multi-branch diff regions, min_depth 2: has permission at Branch South-A'
);

-- Clean up
DELETE FROM authz.usr_role_org_map WHERE usr = 9100;
DELETE FROM authz.role_permission WHERE role = 'test_manager';

SELECT * FROM finish();

ROLLBACK;
