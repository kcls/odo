/**
 * A role's detail path relative to the roles list. Codes are free text,
 * so encode them; the router decodes the :code param.
 */
export const rolePath = (code: string) => encodeURIComponent(code)
