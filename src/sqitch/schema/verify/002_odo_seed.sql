-- Verify odo:002_odo_seed on pg
--
-- The root's identity is a deploy-time variable, so this needs the same
-- defaults the deploy uses. Put overrides in sqitch.conf under
-- [core "variables"] rather than passing -s on the command line, so
-- `sqitch verify` and `sqitch deploy --verify` both see them.
\if :{?root_code}
\else
  \set root_code OLS
\endif

BEGIN;

-- The root exists under whatever code this install deployed.
SELECT 1/COUNT(*) FROM org.unit WHERE code = :'root_code' AND parent IS NULL;

-- ...and there is exactly one root, whatever it is called.
SELECT 1/COUNT(*) FROM org.unit WHERE parent IS NULL AND deleted_at IS NULL;

-- Unit types, platform role, machine account.
SELECT 1/COUNT(*) FROM org.unit_type WHERE label = 'Root';
SELECT 1/COUNT(*) FROM authz.role WHERE code = 'odo-admin';
SELECT 1/COUNT(*) FROM auth.usr WHERE username = 'odo-registration';

ROLLBACK;
