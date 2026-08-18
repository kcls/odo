-- Tests for auth.sync_saml_usr_attrs() trigger,
-- auth.normalize_saml_attr_value(),
-- auth.sync_saml_working_locations(), and authz.sync_saml_auto_roles().

BEGIN;

SELECT plan(52);

-- =============================================================================
-- Seed data
-- =============================================================================

-- Shared org unit hierarchy (see seed/org_unit_data.sql for full tree)
-- Working location tests match against labels
-- 'Test Branch North-A' (9010) and 'Test Branch North-B' (9011)
\i seed/org_unit_data.sql

-- Users
INSERT INTO auth.usr (id, auth_method, username, email) VALUES
    (8000, 'saml', 'saml.user1', 'saml.user1@localhost'),
    (8001, 'saml', 'saml.user2', 'saml.user2@localhost');

-- SAML IDP
INSERT INTO auth.saml_idp_config (id, entity_id, sso_url, name) VALUES
    (8000, 'urn:test:saml:idp:8000', 'https://test-idp/sso', 'Test IDP');

-- SAML identities (user_id is PK and FK to auth.usr)
INSERT INTO auth.usr_saml_identities (user_id, idp_id, name_id) VALUES
    (8000, 8000, 'user1@test-idp'),
    (8001, 8000, 'user2@test-idp');

-- SAML attributes (types provided by the IDP)
-- department and title are plain attributes (no normalizer)
-- location uses split_slash_first to extract e.g. 'Test Branch North-A' from
-- 'Test Branch North-A/Business Application'
INSERT INTO auth.saml_idp_attribute (id, idp, key, label) VALUES
    (8000, 8000, 'department', 'Department'),
    (8001, 8000, 'title', 'Job Title');

INSERT INTO auth.saml_idp_attribute (id, idp, key, label, is_location, normalizer) VALUES
    (8002, 8000, 'Location', 'Location', TRUE, 'split_slash_first'),
    (8003, 8000, 'Location', 'Department (from Location)', FALSE, 'split_slash_last');

-- Roles
INSERT INTO authz.role (code, label, description) VALUES
    ('saml_circ', 'SAML Circ', 'Circulation role for SAML tests'),
    ('saml_cat', 'SAML Cataloging', 'Cataloging role for SAML tests'),
    ('saml_admin', 'SAML Admin', 'Admin role for SAML tests');

-- =============================================================================
-- TEST SUITE T: auth.sync_saml_usr_attrs() trigger
-- =============================================================================
-- The trigger fires AFTER INSERT OR UPDATE OF attributes on
-- auth.usr_saml_identities and decomposes the JSONB attributes
-- into saml_usr_attr rows for each matching saml_idp_attribute config.

-- T.1: UPDATE attributes with configured keys → saml_usr_attr rows created
UPDATE auth.usr_saml_identities
SET attributes = '{"department": "Circulation", "title": "Manager"}'::jsonb
WHERE user_id = 8000;

SELECT is(
    (SELECT COUNT(*)::INTEGER FROM auth.saml_usr_attr WHERE ident = 8000),
    2,
    'T.1 Trigger creates saml_usr_attr rows for configured attribute keys'
);

-- T.2: Verify correct values
SELECT is(
    (SELECT value FROM auth.saml_usr_attr WHERE ident = 8000 AND attr = 8000),
    'Circulation',
    'T.2 department attr has correct value'
);

SELECT is(
    (SELECT value FROM auth.saml_usr_attr WHERE ident = 8000 AND attr = 8001),
    'Manager',
    'T.3 title attr has correct value'
);

-- T.4: Unconfigured attribute keys are ignored
UPDATE auth.usr_saml_identities
SET attributes = '{"department": "Circulation", "title": "Manager", "unknown_key": "ignored"}'::jsonb
WHERE user_id = 8000;

SELECT is(
    (SELECT COUNT(*)::INTEGER FROM auth.saml_usr_attr WHERE ident = 8000),
    2,
    'T.4 Unconfigured attribute keys are ignored (still 2 rows)'
);

-- T.5: Multiple saml_idp_attribute rows with same key get separate saml_usr_attr rows
-- saml_idp_attribute 8002 (Location, split_slash_first) and 8003 (Location, split_slash_last)
UPDATE auth.usr_saml_identities
SET attributes = '{"department": "Circulation", "Location": "Test Branch North-A/IT"}'::jsonb
WHERE user_id = 8000;

SELECT is(
    (SELECT COUNT(*)::INTEGER FROM auth.saml_usr_attr WHERE ident = 8000),
    3,
    'T.5 Same key with different normalizers creates separate rows (dept + 2 Location)'
);

-- T.6: Both Location saml_usr_attr rows have the same raw value
SELECT is(
    (SELECT value FROM auth.saml_usr_attr WHERE ident = 8000 AND attr = 8002),
    'Test Branch North-A/IT',
    'T.6 Location (split_slash_first) has raw value'
);

SELECT is(
    (SELECT value FROM auth.saml_usr_attr WHERE ident = 8000 AND attr = 8003),
    'Test Branch North-A/IT',
    'T.7 Location (split_slash_last) has raw value'
);

-- T.7: Attribute removed from JSONB → saml_usr_attr row deleted
UPDATE auth.usr_saml_identities
SET attributes = '{"department": "Cataloging"}'::jsonb
WHERE user_id = 8000;

SELECT is(
    (SELECT COUNT(*)::INTEGER FROM auth.saml_usr_attr WHERE ident = 8000),
    1,
    'T.8 Removed attributes are pruned (only department remains)'
);

-- Clean up trigger test suite
UPDATE auth.usr_saml_identities
SET attributes = '{}'::jsonb
WHERE user_id = 8000;

DELETE FROM auth.saml_usr_attr WHERE ident = 8000;
DELETE FROM auth.saml_usr_working_location WHERE ident = 8000;

-- =============================================================================
-- TEST SUITE E: End-to-end trigger pipeline
-- =============================================================================
-- Verify that updating attributes JSONB triggers the full pipeline:
-- saml_usr_attr → working locations → role assignments.

-- Set up role mapping: department=Circulation → saml_circ
INSERT INTO authz.saml_attr_role_map (attr, role, attr_value) VALUES
    (8000, 'saml_circ', 'Circulation');

-- Simulate SSO login: update attributes with department + Location
UPDATE auth.usr_saml_identities
SET attributes = '{"department": "Circulation", "Location": "Test Branch North-A/IT"}'::jsonb
WHERE user_id = 8000;

-- E.1: saml_usr_attr rows created (3: department + 2 Location normalizers)
SELECT is(
    (SELECT COUNT(*)::INTEGER FROM auth.saml_usr_attr WHERE ident = 8000),
    3,
    'E.1 Trigger populates saml_usr_attr rows'
);

-- E.2: Working location derived from Location attribute
SELECT ok(
    EXISTS (
        SELECT 1 FROM auth.saml_usr_working_location
        WHERE ident = 8000 AND org_unit = 9010
    ),
    'E.2 Trigger syncs working location (Test Branch North-A)'
);

-- E.3: Role assigned at working location via attribute mapping
SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND org_unit = 9010
          AND is_managed_by_saml = TRUE
    ),
    'E.3 Trigger syncs role assignment at working location'
);

-- E.4: Change location → old role removed, new role added
UPDATE auth.usr_saml_identities
SET attributes = '{"department": "Circulation", "Location": "Test Branch North-B/IT"}'::jsonb
WHERE user_id = 8000;

SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND org_unit = 9011
          AND is_managed_by_saml = TRUE
    ),
    'E.4 Location change: role granted at new location (Branch North-B)'
);

SELECT ok(
    NOT EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND org_unit = 9010
          AND is_managed_by_saml = TRUE
    ),
    'E.5 Location change: role removed from old location (Branch North-A)'
);

-- E.5: Remove department → role revoked
UPDATE auth.usr_saml_identities
SET attributes = '{"Location": "Test Branch North-B/IT"}'::jsonb
WHERE user_id = 8000;

SELECT is(
    (SELECT COUNT(*)::INTEGER FROM authz.usr_role_org_map
     WHERE usr = 8000 AND is_managed_by_saml = TRUE),
    0,
    'E.6 Attribute removed: all SAML-managed roles revoked'
);

-- Clean up end-to-end suite
UPDATE auth.usr_saml_identities
SET attributes = '{}'::jsonb
WHERE user_id = 8000;

DELETE FROM authz.usr_role_org_map WHERE usr = 8000;
DELETE FROM authz.saml_attr_role_map WHERE attr = 8000;
DELETE FROM auth.saml_usr_attr WHERE ident = 8000;
DELETE FROM auth.saml_usr_working_location WHERE ident = 8000;

-- =============================================================================
-- TEST SUITE N: auth.normalize_saml_attr_value()
-- =============================================================================

SELECT is(
    auth.normalize_saml_attr_value('Service Center/Business Application', 'split_slash_first'),
    'Service Center',
    'N.1 split_slash_first extracts first segment'
);

SELECT is(
    auth.normalize_saml_attr_value('Service Center/Business Application', 'split_slash_last'),
    'Business Application',
    'N.2 split_slash_last extracts last segment'
);

SELECT is(
    auth.normalize_saml_attr_value('No Slash Here', 'split_slash_first'),
    'No Slash Here',
    'N.3 split_slash_first with no slash returns full value'
);

SELECT is(
    auth.normalize_saml_attr_value('No Slash Here', 'split_slash_last'),
    'No Slash Here',
    'N.4 split_slash_last with no slash returns full value'
);

SELECT is(
    auth.normalize_saml_attr_value('  Service Center / Dept ', 'split_slash_first'),
    'Service Center',
    'N.5 split_slash_first trims whitespace'
);

SELECT is(
    auth.normalize_saml_attr_value('  Service Center / Dept ', 'split_slash_last'),
    'Dept',
    'N.6 split_slash_last trims whitespace'
);

SELECT is(
    auth.normalize_saml_attr_value('Raw Value', NULL),
    'Raw Value',
    'N.7 NULL normalizer returns value unchanged'
);

SELECT is(
    auth.normalize_saml_attr_value('A/B/C', 'split_slash_first'),
    'A',
    'N.8 split_slash_first with multiple slashes returns first segment'
);

SELECT is(
    auth.normalize_saml_attr_value('A/B/C', 'split_slash_last'),
    'C',
    'N.9 split_slash_last with multiple slashes returns last segment'
);

-- =============================================================================
-- TEST SUITE W: auth.sync_saml_working_locations()
-- =============================================================================

-- user1 has Location='Test Branch North-A/Business Application'
INSERT INTO auth.saml_usr_attr (ident, attr, value) VALUES
    (8000, 8002, 'Test Branch North-A/Business Application');

SELECT auth.sync_saml_working_locations(8000);

SELECT ok(
    EXISTS (
        SELECT 1 FROM auth.saml_usr_working_location
        WHERE ident = 8000 AND org_unit = 9010
    ),
    'W.1 Working location synced: Test Branch North-A matched'
);

SELECT is(
    (SELECT COUNT(*)::INTEGER FROM auth.saml_usr_working_location
     WHERE ident = 8000),
    1,
    'W.2 Exactly 1 working location for user'
);

-- Change location to Test Branch North-B
UPDATE auth.saml_usr_attr SET value = 'Test Branch North-B/IT'
WHERE ident = 8000 AND attr = 8002;

SELECT auth.sync_saml_working_locations(8000);

SELECT ok(
    EXISTS (
        SELECT 1 FROM auth.saml_usr_working_location
        WHERE ident = 8000 AND org_unit = 9011
    ),
    'W.3 After location change: Test Branch North-B matched'
);

SELECT ok(
    NOT EXISTS (
        SELECT 1 FROM auth.saml_usr_working_location
        WHERE ident = 8000 AND org_unit = 9010
    ),
    'W.4 After location change: old Test Branch North-A removed'
);

-- Non-location attribute (8003, split_slash_last) should not affect locations
SELECT is(
    (SELECT COUNT(*)::INTEGER FROM auth.saml_usr_working_location
     WHERE ident = 8000),
    1,
    'W.5 Non-location attribute with same key does not add locations'
);

-- Unrecognized location produces no working locations
UPDATE auth.saml_usr_attr SET value = 'Nonexistent Place/Dept'
WHERE ident = 8000 AND attr = 8002;

SELECT auth.sync_saml_working_locations(8000);

SELECT is(
    (SELECT COUNT(*)::INTEGER FROM auth.saml_usr_working_location
     WHERE ident = 8000),
    0,
    'W.6 Unrecognized location name produces no working locations'
);

-- Clean up
DELETE FROM auth.saml_usr_working_location WHERE ident = 8000;
DELETE FROM auth.saml_usr_attr WHERE ident = 8000;

-- =============================================================================
-- TEST SUITE 1: Basic sync — single attribute, single working location
-- =============================================================================

-- Setup: user1 has department=Circulation, works at Branch North-A
INSERT INTO auth.saml_usr_attr (ident, attr, value) VALUES
    (8000, 8000, 'Circulation');

INSERT INTO auth.saml_usr_working_location (ident, org_unit) VALUES
    (8000, 9010);

-- Map department=Circulation → saml_circ role
INSERT INTO authz.saml_attr_role_map (attr, role, attr_value) VALUES
    (8000, 'saml_circ', 'Circulation');

-- Run sync
SELECT authz.sync_saml_auto_roles(8000);

SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND org_unit = 9010
          AND is_managed_by_saml = TRUE
    ),
    '1.1 Sync adds matching role at working location'
);

-- Verify no manual assignments were created
SELECT ok(
    NOT EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND is_managed_by_saml = FALSE
    ),
    '1.2 Sync does not create manual assignments'
);

-- Run sync again — should be idempotent
SELECT authz.sync_saml_auto_roles(8000);

SELECT is(
    (SELECT COUNT(*)::INTEGER FROM authz.usr_role_org_map
     WHERE usr = 8000 AND role = 'saml_circ' AND is_managed_by_saml = TRUE),
    1,
    '1.3 Re-running sync is idempotent (no duplicates)'
);

-- Clean up suite 1
DELETE FROM authz.usr_role_org_map WHERE usr = 8000;
DELETE FROM authz.saml_attr_role_map WHERE attr = 8000;
DELETE FROM auth.saml_usr_attr WHERE ident = 8000;
DELETE FROM auth.saml_usr_working_location WHERE ident = 8000;

-- =============================================================================
-- TEST SUITE 2: Multiple working locations — role applied at each
-- =============================================================================

INSERT INTO auth.saml_usr_attr (ident, attr, value) VALUES
    (8000, 8000, 'Circulation');

INSERT INTO auth.saml_usr_working_location (ident, org_unit) VALUES
    (8000, 9010),
    (8000, 9011);

INSERT INTO authz.saml_attr_role_map (attr, role, attr_value) VALUES
    (8000, 'saml_circ', 'Circulation');

SELECT authz.sync_saml_auto_roles(8000);

SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND org_unit = 9010
          AND is_managed_by_saml = TRUE
    ),
    '2.1 Role granted at Branch North-A'
);

SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND org_unit = 9011
          AND is_managed_by_saml = TRUE
    ),
    '2.2 Role granted at Branch North-B'
);

SELECT is(
    (SELECT COUNT(*)::INTEGER FROM authz.usr_role_org_map
     WHERE usr = 8000 AND is_managed_by_saml = TRUE),
    2,
    '2.3 Exactly 2 SAML-managed role assignments'
);

-- Clean up suite 2
DELETE FROM authz.usr_role_org_map WHERE usr = 8000;
DELETE FROM authz.saml_attr_role_map WHERE attr = 8000;
DELETE FROM auth.saml_usr_attr WHERE ident = 8000;
DELETE FROM auth.saml_usr_working_location WHERE ident = 8000;

-- =============================================================================
-- TEST SUITE 3: Multiple attributes → multiple roles
-- =============================================================================

-- user1 has department=Circulation AND title=Manager
INSERT INTO auth.saml_usr_attr (ident, attr, value) VALUES
    (8000, 8000, 'Circulation'),
    (8000, 8001, 'Manager');

INSERT INTO auth.saml_usr_working_location (ident, org_unit) VALUES
    (8000, 9010);

-- department=Circulation → saml_circ, title=Manager → saml_admin
INSERT INTO authz.saml_attr_role_map (attr, role, attr_value) VALUES
    (8000, 'saml_circ', 'Circulation'),
    (8001, 'saml_admin', 'Manager');

SELECT authz.sync_saml_auto_roles(8000);

SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND org_unit = 9010
          AND is_managed_by_saml = TRUE
    ),
    '3.1 First attribute mapping grants saml_circ'
);

SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_admin' AND org_unit = 9010
          AND is_managed_by_saml = TRUE
    ),
    '3.2 Second attribute mapping grants saml_admin'
);

SELECT is(
    (SELECT COUNT(*)::INTEGER FROM authz.usr_role_org_map
     WHERE usr = 8000 AND is_managed_by_saml = TRUE),
    2,
    '3.3 Exactly 2 SAML-managed roles from 2 attribute mappings'
);

-- Clean up suite 3
DELETE FROM authz.usr_role_org_map WHERE usr = 8000;
DELETE FROM authz.saml_attr_role_map WHERE attr IN (8000, 8001);
DELETE FROM auth.saml_usr_attr WHERE ident = 8000;
DELETE FROM auth.saml_usr_working_location WHERE ident = 8000;

-- =============================================================================
-- TEST SUITE 4: Stale role removal
-- =============================================================================

-- Initial state: user has department=Circulation at Branch North-A
INSERT INTO auth.saml_usr_attr (ident, attr, value) VALUES
    (8000, 8000, 'Circulation');

INSERT INTO auth.saml_usr_working_location (ident, org_unit) VALUES
    (8000, 9010);

INSERT INTO authz.saml_attr_role_map (attr, role, attr_value) VALUES
    (8000, 'saml_circ', 'Circulation');

SELECT authz.sync_saml_auto_roles(8000);

-- Confirm role is present
SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND is_managed_by_saml = TRUE
    ),
    '4.1 Role exists before attribute change'
);

-- Simulate attribute change: user's department changes to Cataloging
UPDATE auth.saml_usr_attr SET value = 'Cataloging' WHERE ident = 8000 AND attr = 8000;

-- Re-sync
SELECT authz.sync_saml_auto_roles(8000);

SELECT ok(
    NOT EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND is_managed_by_saml = TRUE
    ),
    '4.2 Old role removed after attribute value changes'
);

SELECT is(
    (SELECT COUNT(*)::INTEGER FROM authz.usr_role_org_map
     WHERE usr = 8000 AND is_managed_by_saml = TRUE),
    0,
    '4.3 No SAML-managed roles remain (no mapping for Cataloging)'
);

-- Clean up suite 4
DELETE FROM authz.usr_role_org_map WHERE usr = 8000;
DELETE FROM authz.saml_attr_role_map WHERE attr = 8000;
DELETE FROM auth.saml_usr_attr WHERE ident = 8000;
DELETE FROM auth.saml_usr_working_location WHERE ident = 8000;

-- =============================================================================
-- TEST SUITE 5: Inactive mappings are ignored
-- =============================================================================

INSERT INTO auth.saml_usr_attr (ident, attr, value) VALUES
    (8000, 8000, 'Circulation');

INSERT INTO auth.saml_usr_working_location (ident, org_unit) VALUES
    (8000, 9010);

-- Create an inactive mapping
INSERT INTO authz.saml_attr_role_map (attr, role, attr_value, is_active) VALUES
    (8000, 'saml_circ', 'Circulation', FALSE);

SELECT authz.sync_saml_auto_roles(8000);

SELECT ok(
    NOT EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND is_managed_by_saml = TRUE
    ),
    '5.1 Inactive mapping does not grant role'
);

-- Activate the mapping
UPDATE authz.saml_attr_role_map SET is_active = TRUE
WHERE attr = 8000 AND role = 'saml_circ';

SELECT authz.sync_saml_auto_roles(8000);

SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND is_managed_by_saml = TRUE
    ),
    '5.2 Activating the mapping grants the role on next sync'
);

-- Deactivate the mapping — stale role should be removed
UPDATE authz.saml_attr_role_map SET is_active = FALSE
WHERE attr = 8000 AND role = 'saml_circ';

SELECT authz.sync_saml_auto_roles(8000);

SELECT ok(
    NOT EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND is_managed_by_saml = TRUE
    ),
    '5.3 Deactivating the mapping removes the role on next sync'
);

-- Clean up suite 5
DELETE FROM authz.usr_role_org_map WHERE usr = 8000;
DELETE FROM authz.saml_attr_role_map WHERE attr = 8000;
DELETE FROM auth.saml_usr_attr WHERE ident = 8000;
DELETE FROM auth.saml_usr_working_location WHERE ident = 8000;

-- =============================================================================
-- TEST SUITE 6: Case-insensitive attr_value matching
-- =============================================================================

INSERT INTO auth.saml_usr_attr (ident, attr, value) VALUES
    (8000, 8000, 'circulation');

INSERT INTO auth.saml_usr_working_location (ident, org_unit) VALUES
    (8000, 9010);

-- Mapping uses upper-case value
INSERT INTO authz.saml_attr_role_map (attr, role, attr_value) VALUES
    (8000, 'saml_circ', 'CIRCULATION');

SELECT authz.sync_saml_auto_roles(8000);

SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND is_managed_by_saml = TRUE
    ),
    '6.1 Case-insensitive match: lowercase attr matches uppercase mapping'
);

-- Clean up suite 6
DELETE FROM authz.usr_role_org_map WHERE usr = 8000;
DELETE FROM authz.saml_attr_role_map WHERE attr = 8000;
DELETE FROM auth.saml_usr_attr WHERE ident = 8000;
DELETE FROM auth.saml_usr_working_location WHERE ident = 8000;

-- =============================================================================
-- TEST SUITE 7: Manual assignments are not touched
-- =============================================================================

INSERT INTO auth.saml_usr_attr (ident, attr, value) VALUES
    (8000, 8000, 'Circulation');

INSERT INTO auth.saml_usr_working_location (ident, org_unit) VALUES
    (8000, 9010);

-- Manually assign saml_cat at Branch North-A (is_managed_by_saml = FALSE)
INSERT INTO authz.usr_role_org_map (usr, role, org_unit) VALUES
    (8000, 'saml_cat', 9010);

-- No mapping for saml_cat — sync should not remove the manual assignment
INSERT INTO authz.saml_attr_role_map (attr, role, attr_value) VALUES
    (8000, 'saml_circ', 'Circulation');

SELECT authz.sync_saml_auto_roles(8000);

SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_cat' AND org_unit = 9010
          AND is_managed_by_saml = FALSE
    ),
    '7.1 Manual assignment is preserved after sync'
);

SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND is_managed_by_saml = TRUE
    ),
    '7.2 SAML-managed role is also added alongside manual one'
);

-- Clean up suite 7
DELETE FROM authz.usr_role_org_map WHERE usr = 8000;
DELETE FROM authz.saml_attr_role_map WHERE attr = 8000;
DELETE FROM auth.saml_usr_attr WHERE ident = 8000;
DELETE FROM auth.saml_usr_working_location WHERE ident = 8000;

-- =============================================================================
-- TEST SUITE 8: Working location removal triggers stale role cleanup
-- =============================================================================

INSERT INTO auth.saml_usr_attr (ident, attr, value) VALUES
    (8000, 8000, 'Circulation');

INSERT INTO auth.saml_usr_working_location (ident, org_unit) VALUES
    (8000, 9010),
    (8000, 9011);

INSERT INTO authz.saml_attr_role_map (attr, role, attr_value) VALUES
    (8000, 'saml_circ', 'Circulation');

SELECT authz.sync_saml_auto_roles(8000);

SELECT is(
    (SELECT COUNT(*)::INTEGER FROM authz.usr_role_org_map
     WHERE usr = 8000 AND is_managed_by_saml = TRUE),
    2,
    '8.1 Roles at both working locations before removal'
);

-- Remove Branch North-B as a working location
DELETE FROM auth.saml_usr_working_location
WHERE ident = 8000 AND org_unit = 9011;

SELECT authz.sync_saml_auto_roles(8000);

SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND org_unit = 9010
          AND is_managed_by_saml = TRUE
    ),
    '8.2 Role at remaining location (Branch North-A) is kept'
);

SELECT ok(
    NOT EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND org_unit = 9011
          AND is_managed_by_saml = TRUE
    ),
    '8.3 Role at removed location (Branch North-B) is cleaned up'
);

-- Clean up suite 8
DELETE FROM authz.usr_role_org_map WHERE usr = 8000;
DELETE FROM authz.saml_attr_role_map WHERE attr = 8000;
DELETE FROM auth.saml_usr_attr WHERE ident = 8000;
DELETE FROM auth.saml_usr_working_location WHERE ident = 8000;

-- =============================================================================
-- TEST SUITE 9: One attr value maps to multiple roles
-- =============================================================================

INSERT INTO auth.saml_usr_attr (ident, attr, value) VALUES
    (8000, 8000, 'Circulation');

INSERT INTO auth.saml_usr_working_location (ident, org_unit) VALUES
    (8000, 9010);

-- Same attr_value grants two different roles
INSERT INTO authz.saml_attr_role_map (attr, role, attr_value) VALUES
    (8000, 'saml_circ', 'Circulation'),
    (8000, 'saml_admin', 'Circulation');

SELECT authz.sync_saml_auto_roles(8000);

SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_circ' AND is_managed_by_saml = TRUE
    ),
    '9.1 First role granted from same attr value'
);

SELECT ok(
    EXISTS (
        SELECT 1 FROM authz.usr_role_org_map
        WHERE usr = 8000 AND role = 'saml_admin' AND is_managed_by_saml = TRUE
    ),
    '9.2 Second role granted from same attr value'
);

-- Clean up suite 9
DELETE FROM authz.usr_role_org_map WHERE usr = 8000;
DELETE FROM authz.saml_attr_role_map WHERE attr = 8000;
DELETE FROM auth.saml_usr_attr WHERE ident = 8000;
DELETE FROM auth.saml_usr_working_location WHERE ident = 8000;

SELECT * FROM finish();

ROLLBACK;
