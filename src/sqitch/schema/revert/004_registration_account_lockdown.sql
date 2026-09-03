-- Revert odo:004_registration_account_lockdown from pg

-- Reverting restores the account to active, but deliberately does NOT
-- restore the old published password: nothing should ever put a known
-- credential back on this account. A reverted install still needs
-- src/test-data/ (dev) or scripts/load-data-manifest.sh (anywhere) to get a
-- usable password.

BEGIN;

UPDATE auth.usr
   SET status = 'active',
       updated_at = CURRENT_TIMESTAMP
 WHERE username = 'odo-registration';

COMMIT;
