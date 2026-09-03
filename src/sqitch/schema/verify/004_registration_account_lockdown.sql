-- Verify odo:004_registration_account_lockdown on pg

BEGIN;

-- The account exists and is switched off.
SELECT 1/COUNT(*) FROM auth.usr
 WHERE username = 'odo-registration' AND status = 'inactive';

-- The published seed password no longer authenticates.
SELECT 1/COUNT(*) FROM auth.usr u
  JOIN auth.local_account la ON la.usr = u.id
 WHERE u.username = 'odo-registration'
   AND NOT auth.verify_password('odo-registration-dev-only', la.password_hash);

ROLLBACK;
