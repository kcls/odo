-- Deploy odo:007_drop_make_ban_email_template to pg
-- requires: 001_odo_baseline

-- Drop notification.make_ban_email_template(text, text, text).
--
-- A leftover from before the split: the pre-split schema used it to
-- build Current's ban and trespass notification emails at seed time. The
-- templates now arrive through Current's registration manifest, nothing
-- calls the function (no migration, seed, service or test), and it hard
-- codes one installation's name in the footer. An application's email
-- markup has no business in the platform schema in any case.

BEGIN;

DROP FUNCTION notification.make_ban_email_template(text, text, text);

COMMIT;
