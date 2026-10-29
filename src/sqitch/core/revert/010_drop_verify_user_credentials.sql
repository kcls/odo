-- Revert odo:010_drop_verify_user_credentials from pg
--
-- Restores the function exactly as the baseline defined it, so a revert
-- lands on the baseline's state. It is broken there too.

BEGIN;

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

COMMIT;
