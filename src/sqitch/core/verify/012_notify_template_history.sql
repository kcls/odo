-- Verify odo:012_notify_template_history on pg

BEGIN;

SELECT id, template, version, action, restored_from, code, name,
       description, org_unit, subject_template, body_template,
       body_template_html, sample_data, is_active, deleted_at, changed_by,
       changed_at
  FROM notification.template_version
 WHERE false;

-- The recording trigger is in place. The CASE keeps the aggregate row
-- alive so the division runs (see verify/005).
SELECT 1/(CASE WHEN COUNT(*) = 1 THEN 1 ELSE 0 END)
  FROM pg_trigger
 WHERE tgrelid = 'notification.template'::regclass
   AND tgname = 'record_template_version';

ROLLBACK;
