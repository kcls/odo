-- Revert odo:012_notify_template_history from pg
--
-- Drops the recorded versions with the table: the history kept since the
-- deploy is lost.

BEGIN;

DROP TRIGGER record_template_version ON notification.template;
DROP FUNCTION notification.record_template_version();
DROP TABLE notification.template_version;
DROP FUNCTION notification.refuse_template_version_update();

COMMIT;
