# Odo as a durable source of stable references

Status: **implemented**. This was a prerequisite for decoupling Current onto
its own database, which has since happened (see `docs/app-repo-structure.md`).
Kept as the record of the contract and its rationale.

## Why

Applications reference odo-owned data — Current's `incidents.*` rows carry
org-unit, user, and file references; authz maps roles to org units;
notifications reference templates and email groups. When everything shared one
database, ordinary foreign keys (`ON DELETE NO ACTION`) stopped odo from
hard-deleting a row something still referenced.

That protection was **intra-database**. Now that applications run their own
databases, those cross-database FKs are gone and nothing at the SQL layer stops
odo from hard-deleting a row another service still references — leaving a
dangling reference that can never be resolved again (the name/label would be
gone from the only place it existed).

Rather than rebuild integrity as cross-service machinery, odo upholds a simple,
explicit contract so that a plain stored reference is *always* resolvable:

> **The contract.** Rows in odo's shared tables are **never hard-deleted**. A
> row's identity (uuid) is stable forever. "Deleting" is a soft-delete
> (`deleted_at`), plus an explicit anonymize step for personal data.
> Resolve-by-id APIs return soft-deleted rows (flagged) so historical
> references still render.

With this guarantee, a consumer holding a reference never has to worry that
the referenced entity vanished — the failure mode API-only integrity genuinely
can't handle simply doesn't occur.

## How it's implemented

All of the below is in the baseline schema (`src/sqitch/schema`).

### 1. No hard deletes

`audit.prevent_hard_delete()` — a `BEFORE DELETE` trigger on every shared
table (`org.unit`, `org.unit_type`, `auth.usr`, `asset.file_upload`,
`notification.template`, `notification.email_group`, …) that `RAISE`s.
Deliberate out-of-band surgery has an explicit, greppable escape hatch:
`SET app.allow_hard_delete = 'on'` for the session. (The original design also
called for `REVOKE DELETE` from the application role; the trigger-plus-opt-out
proved sufficient and keeps intent visible in the SQL that uses it. Whoever
overrides it owns fixing up any consumers by hand.)

### 2. Standardized soft-delete

Every shared table carries `deleted_at timestamptz` (and `deleted_by` where
audit matters, e.g. `auth.usr`). Normal reads default to
`WHERE deleted_at IS NULL`; returning deleted rows is opt-in (§4).

### 3. Uniqueness under soft-delete

Business-key uniques are **partial unique indexes `WHERE deleted_at IS
NULL`** (`usr_username_key`, `usr_email_key`, `unit_code_key`,
`unit_label_key`, `email_group_code_key`, `unit_single_root`, …) — only
active rows enforce uniqueness, so a code/label/email/username can be reused
after its holder is soft-deleted. PKs stay fully unique.

### 4. Resolve-by-id returns deleted rows (flagged)

The read contract is split:

- **List / tree / search** endpoints are **active-only** (deleted units must
  not appear in pickers).
- **Resolve-by-id** endpoints return soft-deleted rows too, with a `deleted` /
  `deleted_at` field, so historical references render (e.g. "Kent Library
  (deleted)"): odo-org `label-batch` + unit fetch, odo-auth `get_user`,
  odo-asset `get_files`.

### 5. Personal-data erasure (auth.usr)

"Never hard-delete" collides with right-to-erasure. Resolution:
`auth.anonymize_usr(id, actor)` — an **anonymize-in-place** operation that
nulls/replaces PII (email, names, username) while keeping the row and its
uuid, and sets `deleted_at`/`deleted_by`. References still resolve (to an
anonymized placeholder); the PII is gone. This is the sanctioned way to
"delete a user."

### 6. UUIDs are the cross-database reference

Every shared table carries `uuid uuid NOT NULL DEFAULT gen_random_uuid()`
(unique), exposed throughout the APIs. What the original design left as "a
separate, later decision" was decided during the Current decoupling:
**applications reference odo-owned data exclusively by uuid** — integer ids
never cross the database boundary. Current's columns for org units, users,
and files are all uuid; the JWT `org_unit` claim is a uuid; test fixtures pin
uuids, never sequence values.

Rationale: odo and its consumers are independently backed up, restored, and
reseeded across environments. An integer serial id is only meaningful relative
to odo's sequence — a rebuild, re-key, or cross-environment merge can silently
shift it under a consumer's stored references. A uuid is a stable,
DB-independent identity immune to that (and also avoids id enumeration and
helps event dedup). Integer PKs remain for odo-internal FKs and joins.
