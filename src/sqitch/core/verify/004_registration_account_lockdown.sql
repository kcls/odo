-- Verify odo:004_registration_account_lockdown on pg

BEGIN;

-- The account and its local credential row exist.
--
-- That is deliberately all this checks. 004 leaves the account inactive
-- with an unknowable password, but src/test-data/005_registration_account_dev.sql
-- reverses BOTH facts on purpose -- it sets status back to 'active' and
-- restores the published 'odo-registration-dev-only' password, because the
-- integration and e2e suites log in as this account directly.
--
-- So neither the status nor the password hash is a property of this change
-- on every install: asserting either one turned `sqitch verify` into a
-- permanent failure on any box that had run deploy-test, which made the
-- whole verify run useless as a gate. The production guarantee lives in the
-- deploy script; what survives everywhere is that the account exists and is
-- credential-backed.
SELECT 1/COUNT(*) FROM auth.usr u
  JOIN auth.local_account la ON la.usr = u.id
 WHERE u.username = 'odo-registration';

ROLLBACK;
