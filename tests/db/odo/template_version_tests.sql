-- Tests for notification.record_template_version: every write to
-- notification.template leaves a version, whatever made it.

BEGIN;

SELECT plan(12);

INSERT INTO auth.usr (id, username, email, auth_method, uuid)
VALUES (99701, 'tv_author', 'tv_author@test.local', 'local', gen_random_uuid());

-- Direct SQL with no app.actor_id: an INSERT falls back to created_by.
INSERT INTO notification.template (id, code, name, org_unit, subject_template, body_template, created_by)
VALUES (99701, 'tv-test', 'Versioned', (SELECT id FROM org.root()), 'Hi', 'Body', 99701);

SELECT results_eq(
    'SELECT version, action, changed_by FROM notification.template_version WHERE template = 99701',
    $$VALUES (1, 'create', 99701)$$,
    'An insert is version 1, a create by created_by'
);

-- An update with no actor records no author.
UPDATE notification.template SET body_template = 'Body 2' WHERE id = 99701;
SELECT results_eq(
    'SELECT action, changed_by, body_template FROM notification.template_version WHERE template = 99701 AND version = 2',
    $$VALUES ('update', NULL::integer, 'Body 2')$$,
    'An update outside the API is version 2, author unknown'
);

-- Touching only updated_at isn't a version.
UPDATE notification.template SET updated_at = now() + interval '1 minute' WHERE id = 99701;
SELECT is(
    (SELECT count(*)::integer FROM notification.template_version WHERE template = 99701),
    2,
    'An update that changes only updated_at records nothing'
);

-- Neither is rewriting the same content.
UPDATE notification.template SET body_template = 'Body 2' WHERE id = 99701;
SELECT is(
    (SELECT count(*)::integer FROM notification.template_version WHERE template = 99701),
    2,
    'An update to the same content records nothing'
);

-- The settings a writer uses to name the actor and mark a restore.
DO $$ BEGIN PERFORM set_config('app.actor_id', '99701', true); END $$;
UPDATE notification.template SET sample_data = '{"a": 1}' WHERE id = 99701;
SELECT results_eq(
    'SELECT action, changed_by, restored_from FROM notification.template_version WHERE template = 99701 AND version = 3',
    $$VALUES ('update', 99701, NULL::integer)$$,
    'app.actor_id names the author'
);

DO $$ BEGIN PERFORM set_config('app.restored_from', '1', true); END $$;
UPDATE notification.template SET body_template = 'Body', sample_data = NULL WHERE id = 99701;
SELECT results_eq(
    'SELECT action, restored_from, body_template FROM notification.template_version WHERE template = 99701 AND version = 4',
    $$VALUES ('restore', 1, 'Body')$$,
    'app.restored_from marks a restore'
);
DO $$ BEGIN PERFORM set_config('app.restored_from', '', true); END $$;

UPDATE notification.template SET deleted_at = now() WHERE id = 99701;
SELECT results_eq(
    'SELECT action, deleted_at IS NOT NULL FROM notification.template_version WHERE template = 99701 AND version = 5',
    $$VALUES ('delete', true)$$,
    'A soft delete is recorded as a delete'
);

SELECT is(
    (SELECT array_agg(version ORDER BY version) FROM notification.template_version WHERE template = 99701),
    ARRAY[1, 2, 3, 4, 5],
    'Versions number consecutively'
);

-- Append-only.
SELECT throws_ok(
    'UPDATE notification.template_version SET name = ''tampered'' WHERE template = 99701',
    '23001',
    NULL,
    'Versions cannot be updated'
);
SELECT throws_ok(
    'DELETE FROM notification.template_version WHERE template = 99701',
    '23001',
    NULL,
    'Versions cannot be deleted without the hard-delete override'
);

-- A restore must name its source, and only a restore may.
SELECT throws_ok(
    $$INSERT INTO notification.template_version (template, version, action, code, name, org_unit, subject_template, body_template, is_active)
      VALUES (99701, 99, 'restore', 'x', 'x', 1, 'x', 'x', true)$$,
    '23514',
    NULL,
    'A restore without restored_from is refused'
);
SELECT throws_ok(
    $$INSERT INTO notification.template_version (template, version, action, restored_from, code, name, org_unit, subject_template, body_template, is_active)
      VALUES (99701, 99, 'update', 1, 'x', 'x', 1, 'x', 'x', true)$$,
    '23514',
    NULL,
    'An update with restored_from is refused'
);

SELECT * FROM finish();

ROLLBACK;
