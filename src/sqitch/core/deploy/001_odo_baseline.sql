-- Deploy odo:001_odo_baseline to pg

-- Baseline: the complete odo platform schema (asset, auth, authz,
-- notification, org + the audit helper schema), squashed from the
-- pre-split migration history at the repo split (2026-08). A one-time
-- snapshot (pg_dump of the dev database), not a replay of history. From
-- here the schema evolves via new sqitch changes in this plan.
--
-- Requires the pgcrypto (password hashing, uuid defaults) and pg_trgm
-- (trigram search indexes) extensions; created here, in public, so the
-- deploying role must be allowed to CREATE EXTENSION (or pre-create them).

BEGIN;

--
-- PostgreSQL database dump
--

\restrict BXI02dPOzDO5FBVTzqn2Da4XiMbMdOZOMyVgzphtkBsdbk7lin2nNoMvV5BBKa2

-- Dumped from database version 18.4 (Ubuntu 18.4-1.pgdg24.04+1)
-- Dumped by pg_dump version 18.4 (Ubuntu 18.4-1.pgdg24.04+1)

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: asset; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA asset;


--
-- Name: audit; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA audit;


--
-- Name: SCHEMA audit; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON SCHEMA audit IS 'Audit and logging schema';


--
-- Name: auth; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA auth;


--
-- Name: authz; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA authz;


--
-- Name: notification; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA notification;


--
-- Name: org; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA org;


--
-- Name: pg_trgm; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public;


--
-- Name: EXTENSION pg_trgm; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION pg_trgm IS 'text similarity measurement and index searching based on trigrams';


--
-- Name: pgcrypto; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;


--
-- Name: EXTENSION pgcrypto; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION pgcrypto IS 'cryptographic functions';





--
-- Name: prevent_hard_delete(); Type: FUNCTION; Schema: audit; Owner: -
--

CREATE FUNCTION audit.prevent_hard_delete() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    -- Explicit, greppable opt-out for deliberate hard deletes.
    IF current_setting('app.allow_hard_delete', true) = 'on' THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION
        'hard delete blocked on %.% — soft-delete (set deleted_at) instead, or SET app.allow_hard_delete = ''on'' to override',
        TG_TABLE_SCHEMA, TG_TABLE_NAME
        USING errcode = 'restrict_violation';
END;
$$;


--
-- Name: set_updated_at(); Type: FUNCTION; Schema: audit; Owner: -
--

CREATE FUNCTION audit.set_updated_at() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$;


--
-- Name: anonymize_usr(integer, integer); Type: FUNCTION; Schema: auth; Owner: -
--

CREATE FUNCTION auth.anonymize_usr(p_id integer, p_actor integer DEFAULT NULL::integer) RETURNS void
    LANGUAGE plpgsql
    AS $$
BEGIN
    -- Placeholder (non-PII) names rather than NULL, so the display-name trigger
    -- (auth.usr_set_display_name, fires on name updates for local users)
    -- produces a sensible "Deleted User" instead of an empty string.
    UPDATE auth.usr SET
        email             = 'anon+' || p_id || '@deleted.invalid',
        username          = 'deleted_user_' || p_id,
        first_given_name  = 'Deleted',
        second_given_name = NULL,
        family_name       = 'User',
        display_name      = 'Deleted user',
        deleted_at        = COALESCE(deleted_at, now()),
        deleted_by        = p_actor
    WHERE id = p_id;

    DELETE FROM auth.local_account WHERE usr = p_id;
    DELETE FROM auth.usr_saml_identities WHERE user_id = p_id;
END;
$$;


--
-- Name: hash_password(text); Type: FUNCTION; Schema: auth; Owner: -
--

CREATE FUNCTION auth.hash_password(password text) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    AS $$
BEGIN
    -- Use bcrypt with cost factor 10 (can be adjusted for security/performance tradeoff)
    RETURN crypt(password, gen_salt('bf', 10));
END;
$$;


--
-- Name: normalize_saml_attr_value(text, character varying); Type: FUNCTION; Schema: auth; Owner: -
--

CREATE FUNCTION auth.normalize_saml_attr_value(raw_value text, normalizer character varying) RETURNS text
    LANGUAGE sql IMMUTABLE
    AS $$
    SELECT CASE normalizer
        WHEN 'split_slash_first' THEN TRIM(split_part(raw_value, '/', 1))
        WHEN 'split_slash_last'  THEN TRIM(split_part(raw_value, '/',
            array_length(string_to_array(raw_value, '/'), 1)))
        ELSE raw_value
    END;
$$;


--
-- Name: sync_saml_usr_attrs(); Type: FUNCTION; Schema: auth; Owner: -
--

CREATE FUNCTION auth.sync_saml_usr_attrs() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_attr RECORD;
BEGIN
    -- NEW.idp_id is the integer FK to saml_idp_config.id.

    -- Step 1: Upsert saml_usr_attr for every configured attribute whose
    -- key appears in the JSONB attributes blob.  Multiple saml_idp_attribute
    -- rows sharing the same key (with different normalizers) each receive
    -- a saml_usr_attr row containing the same raw value; normalization
    -- is applied downstream by the sync functions.
    FOR v_attr IN
        SELECT sa.id AS attr_id, sa.key
        FROM auth.saml_idp_attribute sa
        WHERE sa.idp = NEW.idp_id
          AND NEW.attributes ? sa.key
    LOOP
        INSERT INTO auth.saml_usr_attr (ident, attr, value)
        VALUES (NEW.user_id, v_attr.attr_id, NEW.attributes ->> v_attr.key)
        ON CONFLICT (ident, attr)
        DO UPDATE SET value = EXCLUDED.value;
    END LOOP;

    -- Remove saml_usr_attr rows whose attribute key is no longer present
    -- in the JSONB blob (e.g. IdP stopped sending it).
    DELETE FROM auth.saml_usr_attr sua
    WHERE sua.ident = NEW.user_id
      AND NOT EXISTS (
          SELECT 1 FROM auth.saml_idp_attribute sa
          WHERE sa.id = sua.attr
            AND sa.idp = NEW.idp_id
            AND NEW.attributes ? sa.key
      );

    -- Step 2: Sync working locations from location-flagged attributes
    PERFORM auth.sync_saml_working_locations(NEW.user_id);

    -- Step 3: Sync role assignments from attribute-to-role mappings
    PERFORM authz.sync_saml_auto_roles(NEW.user_id);

    RETURN NEW;
END;
$$;


--
-- Name: sync_saml_working_locations(integer); Type: FUNCTION; Schema: auth; Owner: -
--

CREATE FUNCTION auth.sync_saml_working_locations(p_user_id integer) RETURNS void
    LANGUAGE plpgsql
    AS $$
BEGIN
    -- Remove existing saml working locations for this user
    DELETE FROM auth.saml_usr_working_location WHERE ident = p_user_id;

    -- Insert working locations derived from location attributes
    INSERT INTO auth.saml_usr_working_location (ident, org_unit)
    SELECT DISTINCT p_user_id, ou.id
    FROM auth.saml_usr_attr sua
    JOIN auth.saml_idp_attribute sa ON sa.id = sua.attr
        AND sa.is_location = TRUE
    JOIN org.unit ou ON LOWER(ou.label) = LOWER(
        auth.normalize_saml_attr_value(sua.value, sa.normalizer)
    )
    WHERE sua.ident = p_user_id
    ON CONFLICT (ident, org_unit) DO NOTHING;
END;
$$;


--
-- Name: update_user_password(integer, text); Type: FUNCTION; Schema: auth; Owner: -
--

CREATE FUNCTION auth.update_user_password(p_user_id integer, p_new_password text) RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER
    AS $$
BEGIN
    UPDATE auth.local_account
    SET password_hash = auth.hash_password(p_new_password),
        failed_login_attempts = 0,
        locked_until = NULL,
        updated_at = CURRENT_TIMESTAMP
    WHERE usr = p_user_id;
    
    RETURN FOUND;
END;
$$;


--
-- Name: usr_set_display_name(); Type: FUNCTION; Schema: auth; Owner: -
--

CREATE FUNCTION auth.usr_set_display_name() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
	IF NEW.auth_method = 'local' THEN
  		NEW.display_name := TRIM(COALESCE(NEW.first_given_name, '') || ' ' || COALESCE(NEW.family_name, ''));
	END IF;
  	RETURN NEW;
END;
$$;


--
-- Name: verify_password(text, text); Type: FUNCTION; Schema: auth; Owner: -
--

CREATE FUNCTION auth.verify_password(password text, password_hash text) RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER
    AS $$
BEGIN
    -- Returns true if password matches the hash
    RETURN password_hash = crypt(password, password_hash);
END;
$$;


--
-- Name: verify_user_credentials(text, text); Type: FUNCTION; Schema: auth; Owner: -
--

CREATE FUNCTION auth.verify_user_credentials(p_username text, p_password text) RETURNS TABLE(user_id integer, email character varying, username character varying, is_valid boolean)
    LANGUAGE plpgsql SECURITY DEFINER
    AS $$
BEGIN
    RETURN QUERY
    SELECT 
        u.id as user_id,
        u.email,
        u.username,
        auth.verify_password(p_password, la.password_hash) as is_valid
    FROM auth.usr u
    INNER JOIN auth.local_account la ON la.usr = u.id
    WHERE u.username = p_username
        AND u.status = 'active'
        AND u.is_deleted = false
        AND u.auth_method = 'local'
    LIMIT 1;
END;
$$;


--
-- Name: sync_saml_auto_roles(integer); Type: FUNCTION; Schema: authz; Owner: -
--

CREATE FUNCTION authz.sync_saml_auto_roles(p_user_id integer) RETURNS void
    LANGUAGE plpgsql
    AS $$
BEGIN

    -- Add role assignments the user should have but doesn't yet.
    INSERT INTO authz.usr_role_org_map (usr, role, org_unit, is_managed_by_saml)
    SELECT DISTINCT p_user_id, arm.role, wl.org_unit, TRUE
    FROM auth.saml_usr_attr       sua
    JOIN authz.saml_attr_role_map arm ON arm.attr = sua.attr
                                     AND LOWER(arm.attr_value) = LOWER(sua.value)
                                     AND arm.is_active = TRUE
    JOIN auth.saml_usr_working_location wl ON wl.ident = sua.ident
    WHERE sua.ident = p_user_id
    ON CONFLICT (usr, role, org_unit, is_managed_by_saml) DO NOTHING;

    -- Remove stale SAML-managed assignments that no longer match
    -- any active mapping + working-location combination.
    DELETE FROM authz.usr_role_org_map m
    WHERE m.usr = p_user_id
      AND m.is_managed_by_saml = TRUE
      AND NOT EXISTS (
          SELECT 1
          FROM auth.saml_usr_attr       sua
          JOIN authz.saml_attr_role_map arm ON arm.attr = sua.attr
                                           AND LOWER(arm.attr_value) = LOWER(sua.value)
                                           AND arm.is_active = TRUE
          JOIN auth.saml_usr_working_location wl ON wl.ident = sua.ident
          WHERE sua.ident = p_user_id
            AND arm.role   = m.role
            AND wl.org_unit = m.org_unit
      );

END;
$$;


--
-- Name: units_covered_by_grant(integer, integer); Type: FUNCTION; Schema: authz; Owner: -
--

CREATE FUNCTION authz.units_covered_by_grant(p_org_unit integer, p_min_depth integer) RETURNS TABLE(id integer)
    LANGUAGE sql STABLE
    AS $$
    SELECT fpd.id
    FROM org.unit_full_path_at_depth(p_org_unit, p_min_depth) fpd
    WHERE p_min_depth <= (SELECT COUNT(*) - 1 FROM org.unit_ancestors(p_org_unit))
      AND fpd.depth >= p_min_depth
    UNION ALL
    SELECT d.id
    FROM org.unit_descendants(p_org_unit) d
    WHERE p_min_depth > (SELECT COUNT(*) - 1 FROM org.unit_ancestors(p_org_unit))
      AND d.depth >= p_min_depth;
$$;


--
-- Name: usr_covered_units(integer, text); Type: FUNCTION; Schema: authz; Owner: -
--

CREATE FUNCTION authz.usr_covered_units(p_usr_id integer, p_permission_code text) RETURNS TABLE(unit_id integer)
    LANGUAGE sql STABLE
    AS $$
    SELECT DISTINCT covered.id
    FROM authz.role_permission rp
    JOIN authz.usr_role_org_map urom ON urom.role = rp.role
    CROSS JOIN LATERAL authz.units_covered_by_grant(urom.org_unit, rp.min_depth) AS covered
    WHERE rp.perm = p_permission_code
      AND urom.usr = p_usr_id;
$$;


--
-- Name: usr_has_any_role_at(integer, text[], integer); Type: FUNCTION; Schema: authz; Owner: -
--

CREATE FUNCTION authz.usr_has_any_role_at(p_usr_id integer, p_role_codes text[], p_org_unit_id integer DEFAULT NULL::integer) RETURNS boolean
    LANGUAGE plpgsql STABLE
    AS $$
DECLARE
    v_role TEXT;
    v_has_role BOOLEAN;
BEGIN
    -- Check if role array is null or empty
    IF p_role_codes IS NULL OR array_length(p_role_codes, 1) IS NULL THEN
        RETURN FALSE;
    END IF;
    
    -- Loop through each role and check if user has it
    FOREACH v_role IN ARRAY p_role_codes
    LOOP
        -- Use the existing usr_has_role_at function for each role
        v_has_role := authz.usr_has_role_at(p_usr_id, v_role, p_org_unit_id);
        
        -- If user has this role, return true immediately
        IF v_has_role THEN
            RETURN TRUE;
        END IF;
    END LOOP;
    
    -- User doesn't have any of the specified roles
    RETURN FALSE;
END;
$$;


--
-- Name: usr_has_perm_at(integer, text, integer); Type: FUNCTION; Schema: authz; Owner: -
--

CREATE FUNCTION authz.usr_has_perm_at(p_usr_id integer, p_permission_code text, p_org_unit_id integer DEFAULT NULL::integer) RETURNS boolean
    LANGUAGE sql STABLE
    AS $$
    SELECT EXISTS (
        SELECT 1
        FROM authz.role_permission rp
        JOIN authz.usr_role_org_map urom ON urom.role = rp.role
        WHERE rp.perm = p_permission_code
          AND urom.usr = p_usr_id
          AND COALESCE(p_org_unit_id, (SELECT id FROM org.root())) IN (
              SELECT id FROM authz.units_covered_by_grant(urom.org_unit, rp.min_depth)
          )
    );
$$;


--
-- Name: usr_has_role_at(integer, text, integer); Type: FUNCTION; Schema: authz; Owner: -
--

CREATE FUNCTION authz.usr_has_role_at(p_usr_id integer, p_role_code text, p_org_unit_id integer DEFAULT NULL::integer) RETURNS boolean
    LANGUAGE plpgsql STABLE
    AS $$
DECLARE
    v_org_unit_id integer;
    v_has_role BOOLEAN;
BEGIN
    -- If no org unit provided, use the root org unit
    IF p_org_unit_id IS NULL THEN
        SELECT id INTO v_org_unit_id 
        FROM org.unit 
        WHERE parent IS NULL AND deleted_at IS NULL 
        LIMIT 1;
        
        IF v_org_unit_id IS NULL THEN
            RAISE EXCEPTION 'No root organization unit found';
        END IF;
    ELSE
        v_org_unit_id := p_org_unit_id;
    END IF;
    
    -- Check if usr has the role at the specified org unit or any ancestor
    SELECT EXISTS (
        SELECT 1
        FROM authz.usr_role_org_map urm
        WHERE urm.usr = p_usr_id  -- Fixed: was p_user_id
          AND urm.role = p_role_code
          AND urm.org_unit IN (
              SELECT a.id 
              FROM org.unit_ancestors(v_org_unit_id) a
          )
    ) INTO v_has_role;
    
    RETURN v_has_role;
END;
$$;


--
-- Name: usr_perm_scopes(integer); Type: FUNCTION; Schema: authz; Owner: -
--

CREATE FUNCTION authz.usr_perm_scopes(p_usr_id integer) RETURNS TABLE(perm text, is_global boolean, scope_unit_id integer, scope_unit_label text)
    LANGUAGE sql STABLE
    AS $$
    WITH root AS (
        SELECT id FROM org.root() LIMIT 1
    ),
    -- Distinct permissions the user holds via any role.
    perms AS (
        SELECT DISTINCT rp.perm
        FROM authz.role_permission rp
        JOIN authz.usr_role_org_map urom ON urom.role = rp.role
        WHERE urom.usr = p_usr_id
    ),
    -- Covered units per permission.
    covered AS (
        SELECT p.perm, c.unit_id
        FROM perms p
        CROSS JOIN LATERAL authz.usr_covered_units(p_usr_id, p.perm) c
    ),
    -- Whether each permission is effectively global (covers root).
    global AS (
        SELECT p.perm,
               EXISTS (
                   SELECT 1 FROM covered c, root r
                   WHERE c.perm = p.perm AND c.unit_id = r.id
               ) AS is_global
        FROM perms p
    )
    -- Global permissions: a single summary row.
    SELECT g.perm, TRUE AS is_global, NULL::INTEGER, NULL::TEXT
    FROM global g
    WHERE g.is_global

    UNION ALL

    -- Non-global permissions: minimal subtree roots (covered units whose parent
    -- is not covered for the same permission).
    SELECT c.perm, FALSE AS is_global, c.unit_id, u.label
    FROM covered c
    JOIN global g ON g.perm = c.perm AND NOT g.is_global
    JOIN org.unit u ON u.id = c.unit_id
    WHERE NOT EXISTS (
        SELECT 1
        FROM covered parent_c
        WHERE parent_c.perm = c.perm
          AND parent_c.unit_id = u.parent
    );
$$;


--
-- Name: make_ban_email_template(text, text, text); Type: FUNCTION; Schema: notification; Owner: -
--

CREATE FUNCTION notification.make_ban_email_template(p_title text, p_description text, p_button_text text) RETURNS text
    LANGUAGE plpgsql
    AS $$
BEGIN
    RETURN '
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta name="color-scheme" content="light dark">
  <title>' || p_title || '</title>
  <!--[if mso]>
  <noscript>
    <xml>
      <o:OfficeDocumentSettings>
        <o:PixelsPerInch>96</o:PixelsPerInch>
      </o:OfficeDocumentSettings>
    </xml>
  </noscript>
  <![endif]-->
</head>
<body style="margin:0; padding:0; background-color:#f5f5f5; font-family:Segoe UI, Roboto, Helvetica Neue, Helvetica, Arial, sans-serif;">
  <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0" style="background-color:#f5f5f5;">
    <tr>
      <td align="center" style="padding:24px 16px;">
        <table role="presentation" width="520" cellspacing="0" cellpadding="0" border="0" style="max-width:520px; background-color:#ffffff;">
          <tr>
            <td style="padding:24px 32px 16px 32px; border-bottom:1px solid #e5e7eb;">
              <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0">
                <tr>
                  <td>
                    <span style="font-family:Segoe UI, Arial, sans-serif; font-size:20px; font-weight:600; color:#1e3a5f;">Odo Library System</span>
                  </td>
                </tr>
              </table>
            </td>
          </tr>
          <tr>
            <td style="padding:24px 32px;">
              <h1 style="margin:0 0 20px 0; font-family:Segoe UI, Arial, sans-serif; font-size:18px; font-weight:600; color:#111827; line-height:1.4;">
                ' || p_title || '
              </h1>
              <p style="margin:0 0 20px 0; font-family:Segoe UI, Arial, sans-serif; font-size:14px; color:#4b5563; line-height:1.5;">
                ' || p_description || '
              </p>
              <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0" style="background-color:#f9fafb; margin-bottom:24px;">
                <tr>
                  <td style="padding:16px;">
                    <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0">
                      <tr>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px;">
                          <span style="color:#6b7280;">Location</span>
                        </td>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px; color:#111827; text-align:right;">
                          {{location_name}}
                        </td>
                      </tr>
                      <tr>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px; border-top:1px solid #e5e7eb;">
                          <span style="color:#6b7280;">Duration</span>
                        </td>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px; color:#111827; text-align:right; border-top:1px solid #e5e7eb;">
                          {{duration}}
                        </td>
                      </tr>
                      <tr>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px; border-top:1px solid #e5e7eb;">
                          <span style="color:#6b7280;">Start Date</span>
                        </td>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px; color:#111827; text-align:right; border-top:1px solid #e5e7eb;">
                          {{date start_date "%b %d, %Y" timezone}}
                        </td>
                      </tr>
                      <tr>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px; border-top:1px solid #e5e7eb;">
                          <span style="color:#6b7280;">Lift Date</span>
                        </td>
                        <td style="padding:6px 0; font-family:Segoe UI, Arial, sans-serif; font-size:13px; color:#111827; text-align:right; border-top:1px solid #e5e7eb;">
                          {{#if lift_date}}{{date lift_date "%b %d, %Y" timezone}}{{else}}Indefinite{{/if}}
                        </td>
                      </tr>
                    </table>
                  </td>
                </tr>
              </table>
              <table role="presentation" cellspacing="0" cellpadding="0" border="0" align="center" style="margin:0 auto;">
                <tr>
                  <td align="center" bgcolor="#1e3a5f" style="background-color:#1e3a5f; padding:12px 24px; border-radius:6px; -webkit-border-radius:6px; -moz-border-radius:6px;">
                    <a href="{{incident_url}}" target="_blank" style="font-family:Segoe UI, Arial, sans-serif; font-size:14px; font-weight:600; color:#ffffff; text-decoration:none; display:inline-block; mso-line-height-rule:exactly; line-height:20px;">
                      <span style="color:#ffffff;">' || p_button_text || '</span>
                    </a>
                  </td>
                </tr>
              </table>
            </td>
          </tr>
          <tr>
            <td style="padding:16px 32px 24px 32px; border-top:1px solid #e5e7eb;">
              <p style="margin:0; font-family:Segoe UI, Arial, sans-serif; font-size:12px; color:#9ca3af; line-height:1.5;">
                This is an automated message from Current. Please do not reply.
              </p>
              <p style="margin:8px 0 0 0; font-family:Segoe UI, Arial, sans-serif; font-size:11px; color:#9ca3af;">
                King County Library System
              </p>
            </td>
          </tr>
        </table>
      </td>
    </tr>
  </table>
</body>
</html>';
END;
$$;


--
-- Name: update_updated_at_column(); Type: FUNCTION; Schema: notification; Owner: -
--

CREATE FUNCTION notification.update_updated_at_column() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$;


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: unit; Type: TABLE; Schema: org; Owner: -
--

CREATE TABLE org.unit (
    id integer NOT NULL,
    label text NOT NULL,
    code text NOT NULL,
    parent integer,
    unit_type integer NOT NULL,
    deleted_at timestamp with time zone,
    timezone character varying(100),
    uuid uuid DEFAULT gen_random_uuid() NOT NULL
);


--
-- Name: COLUMN unit.timezone; Type: COMMENT; Schema: org; Owner: -
--

COMMENT ON COLUMN org.unit.timezone IS 'IANA timezone identifier (e.g., America/Los_Angeles)';


--
-- Name: root(); Type: FUNCTION; Schema: org; Owner: -
--

CREATE FUNCTION org.root() RETURNS org.unit
    LANGUAGE plpgsql STABLE
    AS $$
DECLARE
    root_unit org.unit;
BEGIN
    SELECT * INTO root_unit
    FROM org.unit
    WHERE parent IS NULL AND deleted_at IS NULL;

    IF root_unit IS NULL THEN
        RAISE EXCEPTION 'No root organization unit found';
    END IF;

    RETURN root_unit;
END;
$$;


--
-- Name: FUNCTION root(); Type: COMMENT; Schema: org; Owner: -
--

COMMENT ON FUNCTION org.root() IS 'Returns the root organization unit (unit with null parent)';


--
-- Name: unit_ancestors(integer); Type: FUNCTION; Schema: org; Owner: -
--

CREATE FUNCTION org.unit_ancestors(unit_id integer) RETURNS TABLE(id integer, label text, code text, parent integer, unit_type integer, depth integer)
    LANGUAGE sql STABLE
    AS $$
WITH RECURSIVE ancestors AS (
    -- Start with the given unit
    SELECT
        u.id,
        u.label,
        u.code,
        u.parent,
        u.unit_type,
        0 as relative_depth
    FROM org.unit u
    WHERE u.id = unit_id AND u.deleted_at IS NULL

    UNION ALL

    -- Recursively get parents
    SELECT
        u.id,
        u.label,
        u.code,
        u.parent,
        u.unit_type,
        a.relative_depth + 1
    FROM org.unit u
    INNER JOIN ancestors a ON u.id = a.parent
    WHERE u.deleted_at IS NULL
),
max_depth_cte AS (
    SELECT MAX(relative_depth) as max_depth FROM ancestors
)
SELECT
    a.id,
    a.label,
    a.code,
    a.parent,
    a.unit_type,
    (SELECT max_depth FROM max_depth_cte) - a.relative_depth as depth
FROM ancestors a
ORDER BY depth;
$$;


--
-- Name: unit_descendants(integer); Type: FUNCTION; Schema: org; Owner: -
--

CREATE FUNCTION org.unit_descendants(unit_id integer) RETURNS TABLE(id integer, label text, code text, parent integer, unit_type integer, depth integer)
    LANGUAGE sql STABLE
    AS $$
WITH RECURSIVE
start_node AS (
    -- Get the absolute depth of the starting node
    SELECT
        u.id,
        u.label,
        u.code,
        u.parent,
        u.unit_type,
        (SELECT COUNT(*) FROM org.unit_ancestors(unit_id)) - 1 as start_depth
    FROM org.unit u
    WHERE u.id = unit_id AND u.deleted_at IS NULL
),
descendants AS (
    -- Start with the given unit
    SELECT
        s.id,
        s.label,
        s.code,
        s.parent,
        s.unit_type,
        0 as relative_depth,
        s.start_depth
    FROM start_node s

    UNION ALL

    -- Recursively get children
    SELECT
        u.id,
        u.label,
        u.code,
        u.parent,
        u.unit_type,
        d.relative_depth + 1,
        d.start_depth
    FROM org.unit u
    INNER JOIN descendants d ON u.parent = d.id
    WHERE u.deleted_at IS NULL
)
SELECT
    id,
    label,
    code,
    parent,
    unit_type,
    start_depth + relative_depth as depth
FROM descendants
ORDER BY depth;
$$;


--
-- Name: unit_full_path(integer); Type: FUNCTION; Schema: org; Owner: -
--

CREATE FUNCTION org.unit_full_path(unit_id integer) RETURNS TABLE(id integer, label text, code text, parent integer, unit_type integer, depth integer)
    LANGUAGE sql STABLE
    AS $$
    -- Get all ancestors (going up the tree) with absolute depth
    SELECT id, label, code, parent, unit_type, depth
    FROM org.unit_ancestors(unit_id)

    UNION

    -- Get all descendants (going down the tree) with absolute depth
    SELECT id, label, code, parent, unit_type, depth
    FROM org.unit_descendants(unit_id)
$$;


--
-- Name: unit_full_path_at_depth(integer, integer); Type: FUNCTION; Schema: org; Owner: -
--

CREATE FUNCTION org.unit_full_path_at_depth(unit_id integer, target_depth integer) RETURNS TABLE(id integer, label text, code text, parent integer, unit_type integer, depth integer)
    LANGUAGE sql STABLE
    AS $$
    WITH ancestor_at_depth AS (
        -- Find the ancestor of unit_id that is at target_depth
        SELECT a.id
        FROM org.unit_ancestors(unit_id) a
        WHERE a.depth = target_depth
        LIMIT 1
    )
    -- Return the ancestor at target_depth plus all its descendants
    SELECT d.id, d.label, d.code, d.parent, d.unit_type, d.depth
    FROM ancestor_at_depth a
    CROSS JOIN LATERAL org.unit_descendants(a.id) d
$$;


--
-- Name: directory; Type: TABLE; Schema: asset; Owner: -
--

CREATE TABLE asset.directory (
    path text NOT NULL,
    read_perm text NOT NULL,
    write_perm text NOT NULL,
    description text
);


--
-- Name: file_upload; Type: TABLE; Schema: asset; Owner: -
--

CREATE TABLE asset.file_upload (
    id integer NOT NULL,
    file_name text NOT NULL,
    file_type text,
    file_size integer,
    storage_path text NOT NULL,
    relative_path text NOT NULL,
    uploaded_by integer NOT NULL,
    uploaded_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    deleted_at timestamp with time zone,
    uuid uuid DEFAULT gen_random_uuid() NOT NULL
);


--
-- Name: file_upload_id_seq; Type: SEQUENCE; Schema: asset; Owner: -
--

CREATE SEQUENCE asset.file_upload_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: file_upload_id_seq; Type: SEQUENCE OWNED BY; Schema: asset; Owner: -
--

ALTER SEQUENCE asset.file_upload_id_seq OWNED BY asset.file_upload.id;


--
-- Name: audit_log; Type: TABLE; Schema: auth; Owner: -
--

CREATE TABLE auth.audit_log (
    id integer NOT NULL,
    user_id integer NOT NULL,
    action character varying(100) NOT NULL,
    resource_type character varying(100),
    resource_id character varying(255),
    ip_address inet,
    user_agent text,
    details jsonb DEFAULT '{}'::jsonb,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
);


--
-- Name: audit_log_id_seq; Type: SEQUENCE; Schema: auth; Owner: -
--

CREATE SEQUENCE auth.audit_log_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: audit_log_id_seq; Type: SEQUENCE OWNED BY; Schema: auth; Owner: -
--

ALTER SEQUENCE auth.audit_log_id_seq OWNED BY auth.audit_log.id;


--
-- Name: local_account; Type: TABLE; Schema: auth; Owner: -
--

CREATE TABLE auth.local_account (
    usr integer CONSTRAINT local_account_user_id_not_null NOT NULL,
    failed_login_attempts integer DEFAULT 0,
    locked_until timestamp with time zone,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    password_hash character varying(255) DEFAULT ('$2a$10$'::text || encode(public.gen_random_bytes(53), 'base64'::text)) NOT NULL,
    id integer NOT NULL
);


--
-- Name: COLUMN local_account.password_hash; Type: COMMENT; Schema: auth; Owner: -
--

COMMENT ON COLUMN auth.local_account.password_hash IS 'BCrypt hash of the user password (using pgcrypto crypt function)';


--
-- Name: local_account_id_seq; Type: SEQUENCE; Schema: auth; Owner: -
--

CREATE SEQUENCE auth.local_account_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: local_account_id_seq; Type: SEQUENCE OWNED BY; Schema: auth; Owner: -
--

ALTER SEQUENCE auth.local_account_id_seq OWNED BY auth.local_account.id;


--
-- Name: saml_auth_requests; Type: TABLE; Schema: auth; Owner: -
--

CREATE TABLE auth.saml_auth_requests (
    id integer NOT NULL,
    request_id character varying(255) NOT NULL,
    idp_id integer NOT NULL,
    relay_state text,
    request_data text NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    expires_at timestamp with time zone DEFAULT (CURRENT_TIMESTAMP + '00:05:00'::interval),
    acs_url text,
    sp_id integer
);


--
-- Name: saml_auth_requests_id_seq; Type: SEQUENCE; Schema: auth; Owner: -
--

CREATE SEQUENCE auth.saml_auth_requests_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: saml_auth_requests_id_seq; Type: SEQUENCE OWNED BY; Schema: auth; Owner: -
--

ALTER SEQUENCE auth.saml_auth_requests_id_seq OWNED BY auth.saml_auth_requests.id;


--
-- Name: saml_idp_attribute; Type: TABLE; Schema: auth; Owner: -
--

CREATE TABLE auth.saml_idp_attribute (
    id integer NOT NULL,
    idp integer NOT NULL,
    key character varying(256) NOT NULL,
    label character varying(256) NOT NULL,
    is_location boolean DEFAULT false NOT NULL,
    normalizer character varying(64)
);


--
-- Name: saml_idp_attribute_id_seq; Type: SEQUENCE; Schema: auth; Owner: -
--

CREATE SEQUENCE auth.saml_idp_attribute_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: saml_idp_attribute_id_seq; Type: SEQUENCE OWNED BY; Schema: auth; Owner: -
--

ALTER SEQUENCE auth.saml_idp_attribute_id_seq OWNED BY auth.saml_idp_attribute.id;


--
-- Name: saml_idp_config; Type: TABLE; Schema: auth; Owner: -
--

CREATE TABLE auth.saml_idp_config (
    id integer NOT NULL,
    entity_id character varying(255) NOT NULL,
    sso_url text,
    slo_url text,
    metadata_url text,
    is_active boolean DEFAULT true,
    attribute_mapping jsonb DEFAULT '{}'::jsonb,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    name character varying(255) NOT NULL,
    session_lifetime_hours integer DEFAULT 8,
    allow_idp_initiated boolean DEFAULT false
);


--
-- Name: saml_idp_config_id_seq; Type: SEQUENCE; Schema: auth; Owner: -
--

CREATE SEQUENCE auth.saml_idp_config_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: saml_idp_config_id_seq; Type: SEQUENCE OWNED BY; Schema: auth; Owner: -
--

ALTER SEQUENCE auth.saml_idp_config_id_seq OWNED BY auth.saml_idp_config.id;


--
-- Name: saml_session; Type: TABLE; Schema: auth; Owner: -
--

CREATE TABLE auth.saml_session (
    id integer NOT NULL,
    session_index character varying(255) NOT NULL,
    idp_id integer NOT NULL,
    name_id character varying(255) NOT NULL,
    session_data jsonb DEFAULT '{}'::jsonb,
    session integer NOT NULL
);


--
-- Name: COLUMN saml_session.session; Type: COMMENT; Schema: auth; Owner: -
--

COMMENT ON COLUMN auth.saml_session.session IS 'Reference to the associated auth session';


--
-- Name: saml_session_id_seq; Type: SEQUENCE; Schema: auth; Owner: -
--

CREATE SEQUENCE auth.saml_session_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: saml_session_id_seq; Type: SEQUENCE OWNED BY; Schema: auth; Owner: -
--

ALTER SEQUENCE auth.saml_session_id_seq OWNED BY auth.saml_session.id;


--
-- Name: saml_sp_config; Type: TABLE; Schema: auth; Owner: -
--

CREATE TABLE auth.saml_sp_config (
    id integer NOT NULL,
    entity_id character varying(255) NOT NULL,
    acs_url text NOT NULL,
    slo_url text,
    x509_cert text NOT NULL,
    private_key text NOT NULL,
    metadata_url text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    callback_url text,
    is_active boolean DEFAULT false NOT NULL,
    idp integer,
    label character varying(255),
    idp_x509_cert text
);


--
-- Name: saml_sp_config_id_seq; Type: SEQUENCE; Schema: auth; Owner: -
--

CREATE SEQUENCE auth.saml_sp_config_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: saml_sp_config_id_seq; Type: SEQUENCE OWNED BY; Schema: auth; Owner: -
--

ALTER SEQUENCE auth.saml_sp_config_id_seq OWNED BY auth.saml_sp_config.id;


--
-- Name: saml_usr_attr; Type: TABLE; Schema: auth; Owner: -
--

CREATE TABLE auth.saml_usr_attr (
    id integer NOT NULL,
    ident integer NOT NULL,
    attr integer NOT NULL,
    value text NOT NULL
);


--
-- Name: saml_usr_attr_id_seq; Type: SEQUENCE; Schema: auth; Owner: -
--

CREATE SEQUENCE auth.saml_usr_attr_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: saml_usr_attr_id_seq; Type: SEQUENCE OWNED BY; Schema: auth; Owner: -
--

ALTER SEQUENCE auth.saml_usr_attr_id_seq OWNED BY auth.saml_usr_attr.id;


--
-- Name: saml_usr_working_location; Type: TABLE; Schema: auth; Owner: -
--

CREATE TABLE auth.saml_usr_working_location (
    id integer NOT NULL,
    ident integer NOT NULL,
    org_unit integer NOT NULL
);


--
-- Name: saml_usr_working_location_id_seq; Type: SEQUENCE; Schema: auth; Owner: -
--

CREATE SEQUENCE auth.saml_usr_working_location_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: saml_usr_working_location_id_seq; Type: SEQUENCE OWNED BY; Schema: auth; Owner: -
--

ALTER SEQUENCE auth.saml_usr_working_location_id_seq OWNED BY auth.saml_usr_working_location.id;


--
-- Name: session; Type: TABLE; Schema: auth; Owner: -
--

CREATE TABLE auth.session (
    id integer NOT NULL,
    usr integer CONSTRAINT session_user_id_not_null NOT NULL,
    token_hash character varying(255) NOT NULL,
    refresh_token_hash character varying(255),
    auth_method text NOT NULL,
    ip_address inet,
    user_agent text,
    is_active boolean DEFAULT true,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    expires_at timestamp with time zone NOT NULL,
    last_activity_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    org_unit integer,
    uuid character varying(36) NOT NULL,
    CONSTRAINT session_auth_method_check CHECK ((auth_method = ANY (ARRAY['local'::text, 'saml'::text, 'oauth'::text])))
);


--
-- Name: COLUMN session.org_unit; Type: COMMENT; Schema: auth; Owner: -
--

COMMENT ON COLUMN auth.session.org_unit IS 'Organization unit context for this session';


--
-- Name: session_id_seq; Type: SEQUENCE; Schema: auth; Owner: -
--

CREATE SEQUENCE auth.session_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: session_id_seq; Type: SEQUENCE OWNED BY; Schema: auth; Owner: -
--

ALTER SEQUENCE auth.session_id_seq OWNED BY auth.session.id;


--
-- Name: usr; Type: TABLE; Schema: auth; Owner: -
--

CREATE TABLE auth.usr (
    id integer NOT NULL,
    email character varying(255) NOT NULL,
    username character varying(100),
    first_given_name character varying(255),
    second_given_name character varying(255),
    family_name character varying(255),
    status text DEFAULT 'active'::text,
    auth_method text NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    last_login_at timestamp with time zone,
    display_name text DEFAULT ''::text NOT NULL,
    deleted_at timestamp with time zone,
    deleted_by integer,
    uuid uuid DEFAULT gen_random_uuid() NOT NULL,
    CONSTRAINT usr_auth_method_check CHECK ((auth_method = ANY (ARRAY['local'::text, 'saml'::text, 'oauth'::text]))),
    CONSTRAINT usr_status_check CHECK ((status = ANY (ARRAY['active'::text, 'inactive'::text, 'suspended'::text, 'deleted'::text])))
);


--
-- Name: usr_id_seq; Type: SEQUENCE; Schema: auth; Owner: -
--

CREATE SEQUENCE auth.usr_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: usr_id_seq; Type: SEQUENCE OWNED BY; Schema: auth; Owner: -
--

ALTER SEQUENCE auth.usr_id_seq OWNED BY auth.usr.id;


--
-- Name: usr_saml_identities; Type: TABLE; Schema: auth; Owner: -
--

CREATE TABLE auth.usr_saml_identities (
    user_id integer NOT NULL,
    idp_id integer NOT NULL,
    name_id character varying(255) NOT NULL,
    attributes jsonb DEFAULT '{}'::jsonb,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    name_id_format character varying(255),
    session_index character varying(255)
);


--
-- Name: permission; Type: TABLE; Schema: authz; Owner: -
--

CREATE TABLE authz.permission (
    code text NOT NULL,
    description text
);


--
-- Name: role; Type: TABLE; Schema: authz; Owner: -
--

CREATE TABLE authz.role (
    code text NOT NULL,
    description text,
    label text NOT NULL
);


--
-- Name: role_permission; Type: TABLE; Schema: authz; Owner: -
--

CREATE TABLE authz.role_permission (
    id integer NOT NULL,
    role text NOT NULL,
    perm text NOT NULL,
    min_depth integer DEFAULT 0 NOT NULL,
    CONSTRAINT role_permission_min_depth_check CHECK ((min_depth >= 0))
);


--
-- Name: role_permission_id_seq; Type: SEQUENCE; Schema: authz; Owner: -
--

CREATE SEQUENCE authz.role_permission_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: role_permission_id_seq; Type: SEQUENCE OWNED BY; Schema: authz; Owner: -
--

ALTER SEQUENCE authz.role_permission_id_seq OWNED BY authz.role_permission.id;


--
-- Name: saml_attr_role_map; Type: TABLE; Schema: authz; Owner: -
--

CREATE TABLE authz.saml_attr_role_map (
    id integer NOT NULL,
    is_active boolean DEFAULT true NOT NULL,
    attr integer NOT NULL,
    role text NOT NULL,
    attr_value text NOT NULL
);


--
-- Name: saml_attr_role_map_id_seq; Type: SEQUENCE; Schema: authz; Owner: -
--

CREATE SEQUENCE authz.saml_attr_role_map_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: saml_attr_role_map_id_seq; Type: SEQUENCE OWNED BY; Schema: authz; Owner: -
--

ALTER SEQUENCE authz.saml_attr_role_map_id_seq OWNED BY authz.saml_attr_role_map.id;


--
-- Name: usr_role_org_map; Type: TABLE; Schema: authz; Owner: -
--

CREATE TABLE authz.usr_role_org_map (
    id integer NOT NULL,
    usr integer NOT NULL,
    role text NOT NULL,
    org_unit integer NOT NULL,
    is_managed_by_saml boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: usr_role_org_map_id_seq; Type: SEQUENCE; Schema: authz; Owner: -
--

CREATE SEQUENCE authz.usr_role_org_map_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: usr_role_org_map_id_seq; Type: SEQUENCE OWNED BY; Schema: authz; Owner: -
--

ALTER SEQUENCE authz.usr_role_org_map_id_seq OWNED BY authz.usr_role_org_map.id;


--
-- Name: delivery; Type: TABLE; Schema: notification; Owner: -
--

CREATE TABLE notification.delivery (
    id bigint NOT NULL,
    event_id bigint CONSTRAINT delivery_message_id_not_null NOT NULL,
    channel character varying(50) NOT NULL,
    template_code character varying(100),
    title_rendered text NOT NULL,
    body_rendered text,
    action_url text,
    status character varying(50) DEFAULT 'pending'::character varying NOT NULL,
    scheduled_for timestamp with time zone,
    processing_started_at timestamp with time zone,
    processing_expires_at timestamp with time zone,
    processing_owner character varying(100),
    processed_at timestamp with time zone,
    retry_count integer DEFAULT 0 NOT NULL,
    max_retries integer DEFAULT 3 NOT NULL,
    next_retry_at timestamp with time zone,
    error_code character varying(100),
    error_message text,
    channel_metadata jsonb,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    recipient_user integer,
    recipient_email_group integer,
    read_at timestamp with time zone,
    dismissed_at timestamp with time zone,
    CONSTRAINT chk_delivery_recipient CHECK ((((recipient_user IS NOT NULL) AND (recipient_email_group IS NULL)) OR ((recipient_user IS NULL) AND (recipient_email_group IS NOT NULL))))
);


--
-- Name: delivery_id_seq; Type: SEQUENCE; Schema: notification; Owner: -
--

CREATE SEQUENCE notification.delivery_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: delivery_id_seq; Type: SEQUENCE OWNED BY; Schema: notification; Owner: -
--

ALTER SEQUENCE notification.delivery_id_seq OWNED BY notification.delivery.id;


--
-- Name: email_group; Type: TABLE; Schema: notification; Owner: -
--

CREATE TABLE notification.email_group (
    id integer NOT NULL,
    code character varying(100) NOT NULL,
    label text NOT NULL,
    is_active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    deleted_at timestamp with time zone,
    uuid uuid DEFAULT gen_random_uuid() NOT NULL
);


--
-- Name: email_group_id_seq; Type: SEQUENCE; Schema: notification; Owner: -
--

CREATE SEQUENCE notification.email_group_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: email_group_id_seq; Type: SEQUENCE OWNED BY; Schema: notification; Owner: -
--

ALTER SEQUENCE notification.email_group_id_seq OWNED BY notification.email_group.id;


--
-- Name: email_group_member; Type: TABLE; Schema: notification; Owner: -
--

CREATE TABLE notification.email_group_member (
    id integer NOT NULL,
    email_group integer NOT NULL,
    email character varying(255) NOT NULL,
    is_active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: email_group_member_id_seq; Type: SEQUENCE; Schema: notification; Owner: -
--

CREATE SEQUENCE notification.email_group_member_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: email_group_member_id_seq; Type: SEQUENCE OWNED BY; Schema: notification; Owner: -
--

ALTER SEQUENCE notification.email_group_member_id_seq OWNED BY notification.email_group_member.id;


--
-- Name: event; Type: TABLE; Schema: notification; Owner: -
--

CREATE TABLE notification.event (
    id bigint CONSTRAINT message_id_not_null NOT NULL,
    dedup_key character varying(255),
    template_code character varying(100),
    template_variables jsonb DEFAULT '{}'::jsonb CONSTRAINT message_template_variables_not_null NOT NULL,
    source_service character varying(100),
    source_entity_type character varying(100),
    source_entity_id bigint,
    created_by integer,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP CONSTRAINT message_created_at_not_null NOT NULL
);


--
-- Name: event_id_seq; Type: SEQUENCE; Schema: notification; Owner: -
--

CREATE SEQUENCE notification.event_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: event_id_seq; Type: SEQUENCE OWNED BY; Schema: notification; Owner: -
--

ALTER SEQUENCE notification.event_id_seq OWNED BY notification.event.id;


--
-- Name: template; Type: TABLE; Schema: notification; Owner: -
--

CREATE TABLE notification.template (
    id integer NOT NULL,
    code character varying(100) NOT NULL,
    name character varying(255) NOT NULL,
    description text,
    org_unit integer DEFAULT 1 NOT NULL,
    subject_template text NOT NULL,
    body_template text NOT NULL,
    sample_data jsonb,
    is_active boolean DEFAULT true NOT NULL,
    created_by integer NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    deleted_at timestamp with time zone,
    body_template_html text,
    uuid uuid DEFAULT gen_random_uuid() NOT NULL
);


--
-- Name: template_id_seq; Type: SEQUENCE; Schema: notification; Owner: -
--

CREATE SEQUENCE notification.template_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: template_id_seq; Type: SEQUENCE OWNED BY; Schema: notification; Owner: -
--

ALTER SEQUENCE notification.template_id_seq OWNED BY notification.template.id;


--
-- Name: user_state; Type: TABLE; Schema: notification; Owner: -
--

CREATE TABLE notification.user_state (
    user_id integer NOT NULL,
    read_watermark_at timestamp with time zone,
    dismiss_watermark_at timestamp with time zone,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: address; Type: TABLE; Schema: org; Owner: -
--

CREATE TABLE org.address (
    id integer NOT NULL,
    org_unit integer NOT NULL,
    address_type text DEFAULT 'physical'::text NOT NULL,
    label text NOT NULL,
    address_line1 text,
    address_line2 text,
    city text,
    state_province text,
    postal_code text,
    deleted_at timestamp with time zone,
    CONSTRAINT address_address_type_check CHECK ((address_type = ANY (ARRAY['physical'::text, 'mailing'::text])))
);


--
-- Name: address_id_seq; Type: SEQUENCE; Schema: org; Owner: -
--

CREATE SEQUENCE org.address_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: address_id_seq; Type: SEQUENCE OWNED BY; Schema: org; Owner: -
--

ALTER SEQUENCE org.address_id_seq OWNED BY org.address.id;


--
-- Name: closure; Type: TABLE; Schema: org; Owner: -
--

CREATE TABLE org.closure (
    id integer NOT NULL,
    org_unit integer NOT NULL,
    closure_date date NOT NULL,
    reason character varying(255) NOT NULL,
    is_emergency boolean DEFAULT false,
    created_by integer,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
);


--
-- Name: closure_id_seq; Type: SEQUENCE; Schema: org; Owner: -
--

CREATE SEQUENCE org.closure_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: closure_id_seq; Type: SEQUENCE OWNED BY; Schema: org; Owner: -
--

ALTER SEQUENCE org.closure_id_seq OWNED BY org.closure.id;


--
-- Name: operating_hours; Type: TABLE; Schema: org; Owner: -
--

CREATE TABLE org.operating_hours (
    id integer NOT NULL,
    org_unit integer NOT NULL,
    day_of_week integer NOT NULL,
    open_time time without time zone NOT NULL,
    close_time time without time zone NOT NULL,
    is_closed boolean DEFAULT false,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_day_of_week CHECK (((day_of_week >= 0) AND (day_of_week <= 6))),
    CONSTRAINT chk_hours CHECK (((close_time > open_time) OR (is_closed = true)))
);


--
-- Name: operating_hours_id_seq; Type: SEQUENCE; Schema: org; Owner: -
--

CREATE SEQUENCE org.operating_hours_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: operating_hours_id_seq; Type: SEQUENCE OWNED BY; Schema: org; Owner: -
--

ALTER SEQUENCE org.operating_hours_id_seq OWNED BY org.operating_hours.id;


--
-- Name: unit_id_seq; Type: SEQUENCE; Schema: org; Owner: -
--

CREATE SEQUENCE org.unit_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: unit_id_seq; Type: SEQUENCE OWNED BY; Schema: org; Owner: -
--

ALTER SEQUENCE org.unit_id_seq OWNED BY org.unit.id;


--
-- Name: unit_type; Type: TABLE; Schema: org; Owner: -
--

CREATE TABLE org.unit_type (
    id integer NOT NULL,
    label text NOT NULL,
    parent integer,
    deleted_at timestamp with time zone,
    can_have_staff boolean DEFAULT true NOT NULL,
    can_have_patrons boolean DEFAULT true NOT NULL,
    uuid uuid DEFAULT gen_random_uuid() NOT NULL
);


--
-- Name: unit_type_id_seq; Type: SEQUENCE; Schema: org; Owner: -
--

CREATE SEQUENCE org.unit_type_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: unit_type_id_seq; Type: SEQUENCE OWNED BY; Schema: org; Owner: -
--

ALTER SEQUENCE org.unit_type_id_seq OWNED BY org.unit_type.id;


--
-- Name: file_upload id; Type: DEFAULT; Schema: asset; Owner: -
--

ALTER TABLE ONLY asset.file_upload ALTER COLUMN id SET DEFAULT nextval('asset.file_upload_id_seq'::regclass);


--
-- Name: audit_log id; Type: DEFAULT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.audit_log ALTER COLUMN id SET DEFAULT nextval('auth.audit_log_id_seq'::regclass);


--
-- Name: local_account id; Type: DEFAULT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.local_account ALTER COLUMN id SET DEFAULT nextval('auth.local_account_id_seq'::regclass);


--
-- Name: saml_auth_requests id; Type: DEFAULT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_auth_requests ALTER COLUMN id SET DEFAULT nextval('auth.saml_auth_requests_id_seq'::regclass);


--
-- Name: saml_idp_attribute id; Type: DEFAULT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_idp_attribute ALTER COLUMN id SET DEFAULT nextval('auth.saml_idp_attribute_id_seq'::regclass);


--
-- Name: saml_idp_config id; Type: DEFAULT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_idp_config ALTER COLUMN id SET DEFAULT nextval('auth.saml_idp_config_id_seq'::regclass);


--
-- Name: saml_session id; Type: DEFAULT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_session ALTER COLUMN id SET DEFAULT nextval('auth.saml_session_id_seq'::regclass);


--
-- Name: saml_sp_config id; Type: DEFAULT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_sp_config ALTER COLUMN id SET DEFAULT nextval('auth.saml_sp_config_id_seq'::regclass);


--
-- Name: saml_usr_attr id; Type: DEFAULT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_usr_attr ALTER COLUMN id SET DEFAULT nextval('auth.saml_usr_attr_id_seq'::regclass);


--
-- Name: saml_usr_working_location id; Type: DEFAULT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_usr_working_location ALTER COLUMN id SET DEFAULT nextval('auth.saml_usr_working_location_id_seq'::regclass);


--
-- Name: session id; Type: DEFAULT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.session ALTER COLUMN id SET DEFAULT nextval('auth.session_id_seq'::regclass);


--
-- Name: usr id; Type: DEFAULT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.usr ALTER COLUMN id SET DEFAULT nextval('auth.usr_id_seq'::regclass);


--
-- Name: role_permission id; Type: DEFAULT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.role_permission ALTER COLUMN id SET DEFAULT nextval('authz.role_permission_id_seq'::regclass);


--
-- Name: saml_attr_role_map id; Type: DEFAULT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.saml_attr_role_map ALTER COLUMN id SET DEFAULT nextval('authz.saml_attr_role_map_id_seq'::regclass);


--
-- Name: usr_role_org_map id; Type: DEFAULT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.usr_role_org_map ALTER COLUMN id SET DEFAULT nextval('authz.usr_role_org_map_id_seq'::regclass);


--
-- Name: delivery id; Type: DEFAULT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.delivery ALTER COLUMN id SET DEFAULT nextval('notification.delivery_id_seq'::regclass);


--
-- Name: email_group id; Type: DEFAULT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.email_group ALTER COLUMN id SET DEFAULT nextval('notification.email_group_id_seq'::regclass);


--
-- Name: email_group_member id; Type: DEFAULT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.email_group_member ALTER COLUMN id SET DEFAULT nextval('notification.email_group_member_id_seq'::regclass);


--
-- Name: event id; Type: DEFAULT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.event ALTER COLUMN id SET DEFAULT nextval('notification.event_id_seq'::regclass);


--
-- Name: template id; Type: DEFAULT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.template ALTER COLUMN id SET DEFAULT nextval('notification.template_id_seq'::regclass);


--
-- Name: address id; Type: DEFAULT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.address ALTER COLUMN id SET DEFAULT nextval('org.address_id_seq'::regclass);


--
-- Name: closure id; Type: DEFAULT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.closure ALTER COLUMN id SET DEFAULT nextval('org.closure_id_seq'::regclass);


--
-- Name: operating_hours id; Type: DEFAULT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.operating_hours ALTER COLUMN id SET DEFAULT nextval('org.operating_hours_id_seq'::regclass);


--
-- Name: unit id; Type: DEFAULT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.unit ALTER COLUMN id SET DEFAULT nextval('org.unit_id_seq'::regclass);


--
-- Name: unit_type id; Type: DEFAULT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.unit_type ALTER COLUMN id SET DEFAULT nextval('org.unit_type_id_seq'::regclass);


--
-- Name: directory directory_pkey; Type: CONSTRAINT; Schema: asset; Owner: -
--

ALTER TABLE ONLY asset.directory
    ADD CONSTRAINT directory_pkey PRIMARY KEY (path);


--
-- Name: file_upload file_upload_pkey; Type: CONSTRAINT; Schema: asset; Owner: -
--

ALTER TABLE ONLY asset.file_upload
    ADD CONSTRAINT file_upload_pkey PRIMARY KEY (id);


--
-- Name: file_upload file_upload_uuid_key; Type: CONSTRAINT; Schema: asset; Owner: -
--

ALTER TABLE ONLY asset.file_upload
    ADD CONSTRAINT file_upload_uuid_key UNIQUE (uuid);


--
-- Name: audit_log audit_log_pkey; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.audit_log
    ADD CONSTRAINT audit_log_pkey PRIMARY KEY (id);


--
-- Name: local_account local_account_pkey; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.local_account
    ADD CONSTRAINT local_account_pkey PRIMARY KEY (id);


--
-- Name: usr_saml_identities saml_account_idp_id_subject_id_key; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.usr_saml_identities
    ADD CONSTRAINT saml_account_idp_id_subject_id_key UNIQUE (idp_id, name_id);


--
-- Name: saml_auth_requests saml_auth_request_request_id_key; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_auth_requests
    ADD CONSTRAINT saml_auth_request_request_id_key UNIQUE (request_id);


--
-- Name: saml_auth_requests saml_auth_requests_pkey; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_auth_requests
    ADD CONSTRAINT saml_auth_requests_pkey PRIMARY KEY (id);


--
-- Name: saml_idp_attribute saml_idp_attribute_pkey; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_idp_attribute
    ADD CONSTRAINT saml_idp_attribute_pkey PRIMARY KEY (id);


--
-- Name: saml_idp_config saml_idp_config_entity_id_key; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_idp_config
    ADD CONSTRAINT saml_idp_config_entity_id_key UNIQUE (entity_id);


--
-- Name: saml_idp_config saml_idp_config_pkey; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_idp_config
    ADD CONSTRAINT saml_idp_config_pkey PRIMARY KEY (id);


--
-- Name: saml_session saml_session_pkey; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_session
    ADD CONSTRAINT saml_session_pkey PRIMARY KEY (id);


--
-- Name: saml_session saml_session_session_index_key; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_session
    ADD CONSTRAINT saml_session_session_index_key UNIQUE (session_index);


--
-- Name: saml_sp_config saml_sp_config_pkey; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_sp_config
    ADD CONSTRAINT saml_sp_config_pkey PRIMARY KEY (id);


--
-- Name: saml_usr_attr saml_usr_attr_pkey; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_usr_attr
    ADD CONSTRAINT saml_usr_attr_pkey PRIMARY KEY (id);


--
-- Name: saml_usr_working_location saml_usr_working_location_pkey; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_usr_working_location
    ADD CONSTRAINT saml_usr_working_location_pkey PRIMARY KEY (id);


--
-- Name: session session_pkey; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.session
    ADD CONSTRAINT session_pkey PRIMARY KEY (id);


--
-- Name: session session_refresh_token_hash_key; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.session
    ADD CONSTRAINT session_refresh_token_hash_key UNIQUE (refresh_token_hash);


--
-- Name: session session_token_hash_key; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.session
    ADD CONSTRAINT session_token_hash_key UNIQUE (token_hash);


--
-- Name: session session_uuid_key; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.session
    ADD CONSTRAINT session_uuid_key UNIQUE (uuid);


--
-- Name: saml_usr_attr uniq_attr_per_ident; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_usr_attr
    ADD CONSTRAINT uniq_attr_per_ident UNIQUE (ident, attr);


--
-- Name: saml_idp_attribute uniq_attr_per_idp; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_idp_attribute
    ADD CONSTRAINT uniq_attr_per_idp UNIQUE (idp, key, normalizer);


--
-- Name: saml_usr_working_location uniq_org_per_ident; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_usr_working_location
    ADD CONSTRAINT uniq_org_per_ident UNIQUE (ident, org_unit);


--
-- Name: usr usr_pkey; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.usr
    ADD CONSTRAINT usr_pkey PRIMARY KEY (id);


--
-- Name: usr_saml_identities usr_saml_identities_pkey; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.usr_saml_identities
    ADD CONSTRAINT usr_saml_identities_pkey PRIMARY KEY (user_id);


--
-- Name: usr usr_uuid_key; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.usr
    ADD CONSTRAINT usr_uuid_key UNIQUE (uuid);


--
-- Name: permission permission_pkey; Type: CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.permission
    ADD CONSTRAINT permission_pkey PRIMARY KEY (code);


--
-- Name: role_permission role_permission_pkey; Type: CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.role_permission
    ADD CONSTRAINT role_permission_pkey PRIMARY KEY (id);


--
-- Name: role_permission role_permission_role_perm_key; Type: CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.role_permission
    ADD CONSTRAINT role_permission_role_perm_key UNIQUE (role, perm);


--
-- Name: role role_pkey; Type: CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.role
    ADD CONSTRAINT role_pkey PRIMARY KEY (code);


--
-- Name: saml_attr_role_map saml_attr_role_map_pkey; Type: CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.saml_attr_role_map
    ADD CONSTRAINT saml_attr_role_map_pkey PRIMARY KEY (id);


--
-- Name: saml_attr_role_map uniq_role_per_attr_value; Type: CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.saml_attr_role_map
    ADD CONSTRAINT uniq_role_per_attr_value UNIQUE (attr, role, attr_value);


--
-- Name: usr_role_org_map usr_role_org_map_pkey; Type: CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.usr_role_org_map
    ADD CONSTRAINT usr_role_org_map_pkey PRIMARY KEY (id);


--
-- Name: usr_role_org_map usr_role_org_map_unique; Type: CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.usr_role_org_map
    ADD CONSTRAINT usr_role_org_map_unique UNIQUE (usr, role, org_unit, is_managed_by_saml);


--
-- Name: delivery delivery_pkey; Type: CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.delivery
    ADD CONSTRAINT delivery_pkey PRIMARY KEY (id);


--
-- Name: email_group_member email_group_member_pkey; Type: CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.email_group_member
    ADD CONSTRAINT email_group_member_pkey PRIMARY KEY (id);


--
-- Name: email_group email_group_pkey; Type: CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.email_group
    ADD CONSTRAINT email_group_pkey PRIMARY KEY (id);


--
-- Name: email_group email_group_uuid_key; Type: CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.email_group
    ADD CONSTRAINT email_group_uuid_key UNIQUE (uuid);


--
-- Name: event message_pkey; Type: CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.event
    ADD CONSTRAINT message_pkey PRIMARY KEY (id);


--
-- Name: template template_code_key; Type: CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.template
    ADD CONSTRAINT template_code_key UNIQUE (code);


--
-- Name: template template_pkey; Type: CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.template
    ADD CONSTRAINT template_pkey PRIMARY KEY (id);


--
-- Name: template template_uuid_key; Type: CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.template
    ADD CONSTRAINT template_uuid_key UNIQUE (uuid);


--
-- Name: email_group_member uq_email_group_member; Type: CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.email_group_member
    ADD CONSTRAINT uq_email_group_member UNIQUE (email_group, email);


--
-- Name: user_state user_state_pkey; Type: CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.user_state
    ADD CONSTRAINT user_state_pkey PRIMARY KEY (user_id);


--
-- Name: address address_label_key; Type: CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.address
    ADD CONSTRAINT address_label_key UNIQUE (label);


--
-- Name: address address_pkey; Type: CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.address
    ADD CONSTRAINT address_pkey PRIMARY KEY (id);


--
-- Name: closure closure_pkey; Type: CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.closure
    ADD CONSTRAINT closure_pkey PRIMARY KEY (id);


--
-- Name: operating_hours operating_hours_pkey; Type: CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.operating_hours
    ADD CONSTRAINT operating_hours_pkey PRIMARY KEY (id);


--
-- Name: unit unit_pkey; Type: CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.unit
    ADD CONSTRAINT unit_pkey PRIMARY KEY (id);


--
-- Name: unit_type unit_type_pkey; Type: CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.unit_type
    ADD CONSTRAINT unit_type_pkey PRIMARY KEY (id);


--
-- Name: unit_type unit_type_uuid_key; Type: CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.unit_type
    ADD CONSTRAINT unit_type_uuid_key UNIQUE (uuid);


--
-- Name: unit unit_uuid_key; Type: CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.unit
    ADD CONSTRAINT unit_uuid_key UNIQUE (uuid);


--
-- Name: auth_usr_display_name_idx; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX auth_usr_display_name_idx ON auth.usr USING btree (display_name);


--
-- Name: auth_usr_display_name_lower_idx; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX auth_usr_display_name_lower_idx ON auth.usr USING btree (lower(display_name));


--
-- Name: auth_usr_display_name_trgm_idx; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX auth_usr_display_name_trgm_idx ON auth.usr USING gin (lower(display_name) public.gin_trgm_ops);


--
-- Name: auth_usr_email_idx; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX auth_usr_email_idx ON auth.usr USING btree (lower((email)::text));


--
-- Name: auth_usr_username_idx; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX auth_usr_username_idx ON auth.usr USING btree (lower((username)::text)) WHERE (username IS NOT NULL);


--
-- Name: idx_audit_log_action; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_audit_log_action ON auth.audit_log USING btree (action);


--
-- Name: idx_audit_log_created_at; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_audit_log_created_at ON auth.audit_log USING btree (created_at);


--
-- Name: idx_audit_log_user_id; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_audit_log_user_id ON auth.audit_log USING btree (user_id);


--
-- Name: idx_auth_session_org_unit; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_auth_session_org_unit ON auth.session USING btree (org_unit);


--
-- Name: idx_local_account_usr; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_local_account_usr ON auth.local_account USING btree (usr);


--
-- Name: idx_saml_auth_request_expires; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_saml_auth_request_expires ON auth.saml_auth_requests USING btree (expires_at);


--
-- Name: idx_saml_idp_attribute_idp; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_saml_idp_attribute_idp ON auth.saml_idp_attribute USING btree (idp);


--
-- Name: idx_saml_session_session; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_saml_session_session ON auth.saml_session USING btree (session);


--
-- Name: idx_saml_usr_attr_attr; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_saml_usr_attr_attr ON auth.saml_usr_attr USING btree (attr);


--
-- Name: idx_saml_usr_attr_ident; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_saml_usr_attr_ident ON auth.saml_usr_attr USING btree (ident);


--
-- Name: idx_saml_usr_working_location_ident; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_saml_usr_working_location_ident ON auth.saml_usr_working_location USING btree (ident);


--
-- Name: idx_saml_usr_working_location_org_unit; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_saml_usr_working_location_org_unit ON auth.saml_usr_working_location USING btree (org_unit);


--
-- Name: idx_session_active; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_session_active ON auth.session USING btree (is_active, expires_at) WHERE (is_active = true);


--
-- Name: idx_session_token_hash; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_session_token_hash ON auth.session USING btree (token_hash);


--
-- Name: idx_session_user_id; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_session_user_id ON auth.session USING btree (usr);


--
-- Name: idx_session_uuid; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_session_uuid ON auth.session USING btree (uuid);


--
-- Name: idx_user_auth_method; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_user_auth_method ON auth.usr USING btree (auth_method);


--
-- Name: idx_user_status; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX idx_user_status ON auth.usr USING btree (status);


--
-- Name: saml_auth_requests_expires_at_idx; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX saml_auth_requests_expires_at_idx ON auth.saml_auth_requests USING btree (expires_at);


--
-- Name: saml_auth_requests_request_id_idx; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX saml_auth_requests_request_id_idx ON auth.saml_auth_requests USING btree (request_id);


--
-- Name: saml_sp_config_active_entity_id; Type: INDEX; Schema: auth; Owner: -
--

CREATE UNIQUE INDEX saml_sp_config_active_entity_id ON auth.saml_sp_config USING btree (entity_id) WHERE (is_active = true);


--
-- Name: usr_email_key; Type: INDEX; Schema: auth; Owner: -
--

CREATE UNIQUE INDEX usr_email_key ON auth.usr USING btree (email) WHERE (deleted_at IS NULL);


--
-- Name: usr_username_key; Type: INDEX; Schema: auth; Owner: -
--

CREATE UNIQUE INDEX usr_username_key ON auth.usr USING btree (username) WHERE (deleted_at IS NULL);


--
-- Name: idx_saml_attr_role_map_attr; Type: INDEX; Schema: authz; Owner: -
--

CREATE INDEX idx_saml_attr_role_map_attr ON authz.saml_attr_role_map USING btree (attr);


--
-- Name: idx_saml_attr_role_map_role; Type: INDEX; Schema: authz; Owner: -
--

CREATE INDEX idx_saml_attr_role_map_role ON authz.saml_attr_role_map USING btree (role);


--
-- Name: idx_usr_role_org_map_org_unit; Type: INDEX; Schema: authz; Owner: -
--

CREATE INDEX idx_usr_role_org_map_org_unit ON authz.usr_role_org_map USING btree (org_unit);


--
-- Name: idx_usr_role_org_map_role; Type: INDEX; Schema: authz; Owner: -
--

CREATE INDEX idx_usr_role_org_map_role ON authz.usr_role_org_map USING btree (role);


--
-- Name: idx_usr_role_org_map_usr; Type: INDEX; Schema: authz; Owner: -
--

CREATE INDEX idx_usr_role_org_map_usr ON authz.usr_role_org_map USING btree (usr);


--
-- Name: email_group_code_key; Type: INDEX; Schema: notification; Owner: -
--

CREATE UNIQUE INDEX email_group_code_key ON notification.email_group USING btree (code) WHERE (deleted_at IS NULL);


--
-- Name: idx_delivery_event_id; Type: INDEX; Schema: notification; Owner: -
--

CREATE INDEX idx_delivery_event_id ON notification.delivery USING btree (event_id);


--
-- Name: idx_delivery_pending_ready; Type: INDEX; Schema: notification; Owner: -
--

CREATE INDEX idx_delivery_pending_ready ON notification.delivery USING btree (channel, next_retry_at NULLS FIRST, scheduled_for NULLS FIRST, created_at) WHERE ((status)::text = 'pending'::text);


--
-- Name: idx_delivery_processing_expired; Type: INDEX; Schema: notification; Owner: -
--

CREATE INDEX idx_delivery_processing_expired ON notification.delivery USING btree (channel, processing_expires_at) WHERE ((status)::text = 'processing'::text);


--
-- Name: idx_delivery_recipient_email_group; Type: INDEX; Schema: notification; Owner: -
--

CREATE INDEX idx_delivery_recipient_email_group ON notification.delivery USING btree (recipient_email_group) WHERE (recipient_email_group IS NOT NULL);


--
-- Name: idx_delivery_recipient_user; Type: INDEX; Schema: notification; Owner: -
--

CREATE INDEX idx_delivery_recipient_user ON notification.delivery USING btree (recipient_user) WHERE (recipient_user IS NOT NULL);


--
-- Name: idx_delivery_user_in_app; Type: INDEX; Schema: notification; Owner: -
--

CREATE INDEX idx_delivery_user_in_app ON notification.delivery USING btree (recipient_user, channel) WHERE ((recipient_user IS NOT NULL) AND ((channel)::text = 'in_app'::text));


--
-- Name: idx_email_group_member_group; Type: INDEX; Schema: notification; Owner: -
--

CREATE INDEX idx_email_group_member_group ON notification.email_group_member USING btree (email_group) WHERE (is_active = true);


--
-- Name: idx_notification_template_active; Type: INDEX; Schema: notification; Owner: -
--

CREATE INDEX idx_notification_template_active ON notification.template USING btree (is_active) WHERE (deleted_at IS NULL);


--
-- Name: idx_notification_template_code; Type: INDEX; Schema: notification; Owner: -
--

CREATE INDEX idx_notification_template_code ON notification.template USING btree (code) WHERE (deleted_at IS NULL);


--
-- Name: idx_notification_template_org_unit; Type: INDEX; Schema: notification; Owner: -
--

CREATE INDEX idx_notification_template_org_unit ON notification.template USING btree (org_unit);


--
-- Name: uq_delivery_event_channel_email_group; Type: INDEX; Schema: notification; Owner: -
--

CREATE UNIQUE INDEX uq_delivery_event_channel_email_group ON notification.delivery USING btree (event_id, channel, recipient_email_group) WHERE (recipient_email_group IS NOT NULL);


--
-- Name: uq_delivery_event_channel_user; Type: INDEX; Schema: notification; Owner: -
--

CREATE UNIQUE INDEX uq_delivery_event_channel_user ON notification.delivery USING btree (event_id, channel, recipient_user) WHERE (recipient_user IS NOT NULL);


--
-- Name: idx_address_org_unit; Type: INDEX; Schema: org; Owner: -
--

CREATE INDEX idx_address_org_unit ON org.address USING btree (org_unit) WHERE (deleted_at IS NULL);


--
-- Name: idx_closure_date; Type: INDEX; Schema: org; Owner: -
--

CREATE INDEX idx_closure_date ON org.closure USING btree (closure_date);


--
-- Name: idx_closure_org_unit_date; Type: INDEX; Schema: org; Owner: -
--

CREATE INDEX idx_closure_org_unit_date ON org.closure USING btree (org_unit, closure_date);


--
-- Name: idx_operating_hours_day; Type: INDEX; Schema: org; Owner: -
--

CREATE INDEX idx_operating_hours_day ON org.operating_hours USING btree (org_unit, day_of_week);


--
-- Name: idx_operating_hours_org_unit; Type: INDEX; Schema: org; Owner: -
--

CREATE INDEX idx_operating_hours_org_unit ON org.operating_hours USING btree (org_unit);


--
-- Name: idx_unit_parent; Type: INDEX; Schema: org; Owner: -
--

CREATE INDEX idx_unit_parent ON org.unit USING btree (parent) WHERE (deleted_at IS NULL);


--
-- Name: idx_unit_type; Type: INDEX; Schema: org; Owner: -
--

CREATE INDEX idx_unit_type ON org.unit USING btree (unit_type) WHERE (deleted_at IS NULL);


--
-- Name: idx_unit_type_parent; Type: INDEX; Schema: org; Owner: -
--

CREATE INDEX idx_unit_type_parent ON org.unit_type USING btree (parent) WHERE (deleted_at IS NULL);


--
-- Name: unit_code_key; Type: INDEX; Schema: org; Owner: -
--

CREATE UNIQUE INDEX unit_code_key ON org.unit USING btree (code) WHERE (deleted_at IS NULL);


--
-- Name: unit_label_key; Type: INDEX; Schema: org; Owner: -
--

CREATE UNIQUE INDEX unit_label_key ON org.unit USING btree (label) WHERE (deleted_at IS NULL);


--
-- Name: unit_single_root; Type: INDEX; Schema: org; Owner: -
--

CREATE UNIQUE INDEX unit_single_root ON org.unit USING btree (((parent IS NULL))) WHERE ((parent IS NULL) AND (deleted_at IS NULL));


--
-- Name: unit_type_label_key; Type: INDEX; Schema: org; Owner: -
--

CREATE UNIQUE INDEX unit_type_label_key ON org.unit_type USING btree (label) WHERE (deleted_at IS NULL);


--
-- Name: file_upload prevent_hard_delete; Type: TRIGGER; Schema: asset; Owner: -
--

CREATE TRIGGER prevent_hard_delete BEFORE DELETE ON asset.file_upload FOR EACH ROW EXECUTE FUNCTION audit.prevent_hard_delete();


--
-- Name: usr prevent_hard_delete; Type: TRIGGER; Schema: auth; Owner: -
--

CREATE TRIGGER prevent_hard_delete BEFORE DELETE ON auth.usr FOR EACH ROW EXECUTE FUNCTION audit.prevent_hard_delete();


--
-- Name: usr_saml_identities trg_sync_saml_usr_attrs; Type: TRIGGER; Schema: auth; Owner: -
--

CREATE TRIGGER trg_sync_saml_usr_attrs AFTER INSERT OR UPDATE OF attributes ON auth.usr_saml_identities FOR EACH ROW EXECUTE FUNCTION auth.sync_saml_usr_attrs();


--
-- Name: local_account update_local_account_updated_at; Type: TRIGGER; Schema: auth; Owner: -
--

CREATE TRIGGER update_local_account_updated_at BEFORE UPDATE ON auth.local_account FOR EACH ROW EXECUTE FUNCTION audit.set_updated_at();


--
-- Name: saml_idp_config update_saml_idp_config_updated_at; Type: TRIGGER; Schema: auth; Owner: -
--

CREATE TRIGGER update_saml_idp_config_updated_at BEFORE UPDATE ON auth.saml_idp_config FOR EACH ROW EXECUTE FUNCTION audit.set_updated_at();


--
-- Name: saml_sp_config update_saml_sp_config_updated_at; Type: TRIGGER; Schema: auth; Owner: -
--

CREATE TRIGGER update_saml_sp_config_updated_at BEFORE UPDATE ON auth.saml_sp_config FOR EACH ROW EXECUTE FUNCTION audit.set_updated_at();


--
-- Name: usr update_user_updated_at; Type: TRIGGER; Schema: auth; Owner: -
--

CREATE TRIGGER update_user_updated_at BEFORE UPDATE ON auth.usr FOR EACH ROW EXECUTE FUNCTION audit.set_updated_at();


--
-- Name: usr_saml_identities update_usr_saml_identities_updated_at; Type: TRIGGER; Schema: auth; Owner: -
--

CREATE TRIGGER update_usr_saml_identities_updated_at BEFORE UPDATE ON auth.usr_saml_identities FOR EACH ROW EXECUTE FUNCTION audit.set_updated_at();


--
-- Name: usr usr_set_display_name_trigger; Type: TRIGGER; Schema: auth; Owner: -
--

CREATE TRIGGER usr_set_display_name_trigger BEFORE INSERT OR UPDATE OF first_given_name, family_name ON auth.usr FOR EACH ROW EXECUTE FUNCTION auth.usr_set_display_name();


--
-- Name: email_group prevent_hard_delete; Type: TRIGGER; Schema: notification; Owner: -
--

CREATE TRIGGER prevent_hard_delete BEFORE DELETE ON notification.email_group FOR EACH ROW EXECUTE FUNCTION audit.prevent_hard_delete();


--
-- Name: template prevent_hard_delete; Type: TRIGGER; Schema: notification; Owner: -
--

CREATE TRIGGER prevent_hard_delete BEFORE DELETE ON notification.template FOR EACH ROW EXECUTE FUNCTION audit.prevent_hard_delete();


--
-- Name: delivery update_notification_delivery_updated_at; Type: TRIGGER; Schema: notification; Owner: -
--

CREATE TRIGGER update_notification_delivery_updated_at BEFORE UPDATE ON notification.delivery FOR EACH ROW EXECUTE FUNCTION notification.update_updated_at_column();


--
-- Name: template update_notification_template_updated_at; Type: TRIGGER; Schema: notification; Owner: -
--

CREATE TRIGGER update_notification_template_updated_at BEFORE UPDATE ON notification.template FOR EACH ROW EXECUTE FUNCTION notification.update_updated_at_column();


--
-- Name: user_state update_notification_user_state_updated_at; Type: TRIGGER; Schema: notification; Owner: -
--

CREATE TRIGGER update_notification_user_state_updated_at BEFORE UPDATE ON notification.user_state FOR EACH ROW EXECUTE FUNCTION notification.update_updated_at_column();


--
-- Name: unit prevent_hard_delete; Type: TRIGGER; Schema: org; Owner: -
--

CREATE TRIGGER prevent_hard_delete BEFORE DELETE ON org.unit FOR EACH ROW EXECUTE FUNCTION audit.prevent_hard_delete();


--
-- Name: unit_type prevent_hard_delete; Type: TRIGGER; Schema: org; Owner: -
--

CREATE TRIGGER prevent_hard_delete BEFORE DELETE ON org.unit_type FOR EACH ROW EXECUTE FUNCTION audit.prevent_hard_delete();


--
-- Name: operating_hours update_operating_hours_updated_at; Type: TRIGGER; Schema: org; Owner: -
--

CREATE TRIGGER update_operating_hours_updated_at BEFORE UPDATE ON org.operating_hours FOR EACH ROW EXECUTE FUNCTION audit.set_updated_at();


--
-- Name: directory directory_read_perm_fkey; Type: FK CONSTRAINT; Schema: asset; Owner: -
--

ALTER TABLE ONLY asset.directory
    ADD CONSTRAINT directory_read_perm_fkey FOREIGN KEY (read_perm) REFERENCES authz.permission(code) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: directory directory_write_perm_fkey; Type: FK CONSTRAINT; Schema: asset; Owner: -
--

ALTER TABLE ONLY asset.directory
    ADD CONSTRAINT directory_write_perm_fkey FOREIGN KEY (write_perm) REFERENCES authz.permission(code) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: file_upload file_upload_uploaded_by_fkey; Type: FK CONSTRAINT; Schema: asset; Owner: -
--

ALTER TABLE ONLY asset.file_upload
    ADD CONSTRAINT file_upload_uploaded_by_fkey FOREIGN KEY (uploaded_by) REFERENCES auth.usr(id);


--
-- Name: audit_log audit_log_user_id_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.audit_log
    ADD CONSTRAINT audit_log_user_id_fkey FOREIGN KEY (user_id) REFERENCES auth.usr(id);


--
-- Name: local_account local_account_usr_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.local_account
    ADD CONSTRAINT local_account_usr_fkey FOREIGN KEY (usr) REFERENCES auth.usr(id) ON DELETE CASCADE;


--
-- Name: saml_auth_requests saml_auth_requests_idp_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_auth_requests
    ADD CONSTRAINT saml_auth_requests_idp_fkey FOREIGN KEY (idp_id) REFERENCES auth.saml_idp_config(id);


--
-- Name: saml_auth_requests saml_auth_requests_sp_id_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_auth_requests
    ADD CONSTRAINT saml_auth_requests_sp_id_fkey FOREIGN KEY (sp_id) REFERENCES auth.saml_sp_config(id);


--
-- Name: saml_idp_attribute saml_idp_attribute_idp_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_idp_attribute
    ADD CONSTRAINT saml_idp_attribute_idp_fkey FOREIGN KEY (idp) REFERENCES auth.saml_idp_config(id);


--
-- Name: saml_session saml_session_idp_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_session
    ADD CONSTRAINT saml_session_idp_fkey FOREIGN KEY (idp_id) REFERENCES auth.saml_idp_config(id);


--
-- Name: saml_session saml_session_session_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_session
    ADD CONSTRAINT saml_session_session_fkey FOREIGN KEY (session) REFERENCES auth.session(id) ON DELETE CASCADE;


--
-- Name: saml_sp_config saml_sp_config_idp_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_sp_config
    ADD CONSTRAINT saml_sp_config_idp_fkey FOREIGN KEY (idp) REFERENCES auth.saml_idp_config(id);


--
-- Name: saml_usr_attr saml_usr_attr_attr_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_usr_attr
    ADD CONSTRAINT saml_usr_attr_attr_fkey FOREIGN KEY (attr) REFERENCES auth.saml_idp_attribute(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: saml_usr_attr saml_usr_attr_ident_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_usr_attr
    ADD CONSTRAINT saml_usr_attr_ident_fkey FOREIGN KEY (ident) REFERENCES auth.usr_saml_identities(user_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: saml_usr_working_location saml_usr_working_location_ident_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_usr_working_location
    ADD CONSTRAINT saml_usr_working_location_ident_fkey FOREIGN KEY (ident) REFERENCES auth.usr_saml_identities(user_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: saml_usr_working_location saml_usr_working_location_org_unit_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.saml_usr_working_location
    ADD CONSTRAINT saml_usr_working_location_org_unit_fkey FOREIGN KEY (org_unit) REFERENCES org.unit(id);


--
-- Name: session session_org_unit_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.session
    ADD CONSTRAINT session_org_unit_fkey FOREIGN KEY (org_unit) REFERENCES org.unit(id) ON DELETE RESTRICT;


--
-- Name: session session_user_id_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.session
    ADD CONSTRAINT session_user_id_fkey FOREIGN KEY (usr) REFERENCES auth.usr(id);


--
-- Name: usr_saml_identities usr_saml_identities_idp_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.usr_saml_identities
    ADD CONSTRAINT usr_saml_identities_idp_fkey FOREIGN KEY (idp_id) REFERENCES auth.saml_idp_config(id);


--
-- Name: usr_saml_identities usr_saml_identities_user_id_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.usr_saml_identities
    ADD CONSTRAINT usr_saml_identities_user_id_fkey FOREIGN KEY (user_id) REFERENCES auth.usr(id);


--
-- Name: role_permission role_permission_perm_fkey; Type: FK CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.role_permission
    ADD CONSTRAINT role_permission_perm_fkey FOREIGN KEY (perm) REFERENCES authz.permission(code) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: role_permission role_permission_role_fkey; Type: FK CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.role_permission
    ADD CONSTRAINT role_permission_role_fkey FOREIGN KEY (role) REFERENCES authz.role(code) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: saml_attr_role_map saml_attr_role_map_attr_fkey; Type: FK CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.saml_attr_role_map
    ADD CONSTRAINT saml_attr_role_map_attr_fkey FOREIGN KEY (attr) REFERENCES auth.saml_idp_attribute(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: saml_attr_role_map saml_attr_role_map_role_fkey; Type: FK CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.saml_attr_role_map
    ADD CONSTRAINT saml_attr_role_map_role_fkey FOREIGN KEY (role) REFERENCES authz.role(code) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: usr_role_org_map usr_role_org_map_org_unit_fkey; Type: FK CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.usr_role_org_map
    ADD CONSTRAINT usr_role_org_map_org_unit_fkey FOREIGN KEY (org_unit) REFERENCES org.unit(id);


--
-- Name: usr_role_org_map usr_role_org_map_role_fkey; Type: FK CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.usr_role_org_map
    ADD CONSTRAINT usr_role_org_map_role_fkey FOREIGN KEY (role) REFERENCES authz.role(code);


--
-- Name: usr_role_org_map usr_role_org_map_usr_fkey; Type: FK CONSTRAINT; Schema: authz; Owner: -
--

ALTER TABLE ONLY authz.usr_role_org_map
    ADD CONSTRAINT usr_role_org_map_usr_fkey FOREIGN KEY (usr) REFERENCES auth.usr(id);


--
-- Name: delivery delivery_message_id_fkey; Type: FK CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.delivery
    ADD CONSTRAINT delivery_message_id_fkey FOREIGN KEY (event_id) REFERENCES notification.event(id) ON DELETE CASCADE;


--
-- Name: delivery delivery_recipient_email_group_fkey; Type: FK CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.delivery
    ADD CONSTRAINT delivery_recipient_email_group_fkey FOREIGN KEY (recipient_email_group) REFERENCES notification.email_group(id);


--
-- Name: delivery delivery_recipient_user_fkey; Type: FK CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.delivery
    ADD CONSTRAINT delivery_recipient_user_fkey FOREIGN KEY (recipient_user) REFERENCES auth.usr(id);


--
-- Name: email_group_member email_group_member_email_group_fkey; Type: FK CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.email_group_member
    ADD CONSTRAINT email_group_member_email_group_fkey FOREIGN KEY (email_group) REFERENCES notification.email_group(id) ON DELETE CASCADE;


--
-- Name: event message_created_by_fkey; Type: FK CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.event
    ADD CONSTRAINT message_created_by_fkey FOREIGN KEY (created_by) REFERENCES auth.usr(id);


--
-- Name: template template_created_by_fkey; Type: FK CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.template
    ADD CONSTRAINT template_created_by_fkey FOREIGN KEY (created_by) REFERENCES auth.usr(id);


--
-- Name: template template_org_unit_fkey; Type: FK CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.template
    ADD CONSTRAINT template_org_unit_fkey FOREIGN KEY (org_unit) REFERENCES org.unit(id);


--
-- Name: user_state user_state_user_id_fkey; Type: FK CONSTRAINT; Schema: notification; Owner: -
--

ALTER TABLE ONLY notification.user_state
    ADD CONSTRAINT user_state_user_id_fkey FOREIGN KEY (user_id) REFERENCES auth.usr(id);


--
-- Name: address address_org_unit_fkey; Type: FK CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.address
    ADD CONSTRAINT address_org_unit_fkey FOREIGN KEY (org_unit) REFERENCES org.unit(id);


--
-- Name: closure closure_created_by_fkey; Type: FK CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.closure
    ADD CONSTRAINT closure_created_by_fkey FOREIGN KEY (created_by) REFERENCES auth.usr(id);


--
-- Name: closure closure_org_unit_fkey; Type: FK CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.closure
    ADD CONSTRAINT closure_org_unit_fkey FOREIGN KEY (org_unit) REFERENCES org.unit(id);


--
-- Name: operating_hours operating_hours_org_unit_fkey; Type: FK CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.operating_hours
    ADD CONSTRAINT operating_hours_org_unit_fkey FOREIGN KEY (org_unit) REFERENCES org.unit(id);


--
-- Name: unit unit_parent_fkey; Type: FK CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.unit
    ADD CONSTRAINT unit_parent_fkey FOREIGN KEY (parent) REFERENCES org.unit(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: unit_type unit_type_parent_fkey; Type: FK CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.unit_type
    ADD CONSTRAINT unit_type_parent_fkey FOREIGN KEY (parent) REFERENCES org.unit_type(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: unit unit_unit_type_fkey; Type: FK CONSTRAINT; Schema: org; Owner: -
--

ALTER TABLE ONLY org.unit
    ADD CONSTRAINT unit_unit_type_fkey FOREIGN KEY (unit_type) REFERENCES org.unit_type(id) DEFERRABLE INITIALLY DEFERRED;


--
-- PostgreSQL database dump complete
--

\unrestrict BXI02dPOzDO5FBVTzqn2Da4XiMbMdOZOMyVgzphtkBsdbk7lin2nNoMvV5BBKa2


COMMIT;
