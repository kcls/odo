-- A login-only role for e2e users and role-assignment lifecycle tests.
-- Tests must not depend on app-registered roles, so the fixture set
-- carries its own assignable role. It grants exactly odo.auth.session
-- (logging in requires that permission) and nothing else.
--
-- Idempotent: safe to re-run.

BEGIN;

INSERT INTO authz.role (code, label, description)
VALUES ('e2e-test-role', 'E2E Test Role',
        'Login-only role (odo.auth.session) used by e2e users and role-assignment integration tests')
ON CONFLICT (code) DO NOTHING;

INSERT INTO authz.role_permission (role, perm, min_depth)
SELECT 'e2e-test-role', 'odo.auth.session', 0
 WHERE NOT EXISTS (SELECT 1 FROM authz.role_permission
                    WHERE role = 'e2e-test-role' AND perm = 'odo.auth.session');

COMMIT;
