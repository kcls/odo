import { Loader2Icon, LogInIcon, TriangleAlertIcon } from "lucide-react"
import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { redirect, useLoaderData, useNavigate, type LoaderFunctionArgs } from "react-router"

import { session } from "@/auth/session"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldGroup, FieldLabel, FieldSeparator } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { loadNamespaces } from "@/i18n"
import { safeRedirectTarget } from "@/lib/loader"

/** SAML callback failures arrive as ?error=<code>. */
const SAML_ERRORS = ["no_roles", "saml_auth_failed", "saml_no_token", "saml_token_invalid"] as const
type SamlError = (typeof SAML_ERRORS)[number]
type LoginError = SamlError | "generic"

function loginError(code: string | null): LoginError | null {
  if (code === null) return null
  return SAML_ERRORS.includes(code as SamlError) ? (code as SamlError) : "generic"
}

export async function loader({ request }: LoaderFunctionArgs) {
  const params = new URL(request.url).searchParams
  const redirectTo = safeRedirectTarget(params.get("redirect_to"))
  await loadNamespaces("login")

  let error = loginError(params.get("error"))
  if (!error && params.get("sso") === "1") {
    // The SAML ACS set the HttpOnly refresh cookie and sent the user
    // back here; exchange it for an access token.
    try {
      await session.completeSamlLogin()
      return redirect(redirectTo)
    } catch {
      error = "saml_token_invalid"
    }
  } else if (!error && session.isAuthenticated()) {
    return redirect(redirectTo)
  }

  // SSO is optional; failures fall back to the local login form.
  const ssoConfigs = await session.listSSOConfigs().catch((err) => {
    console.error("Failed to fetch SSO configs:", err)
    return []
  })

  return {
    redirectTo,
    ssoConfigs,
    error,
    sessionExpired: params.get("session_expired") === "1",
  }
}

export function Component() {
  const { redirectTo, ssoConfigs, error: initialError, sessionExpired } = useLoaderData<typeof loader>()
  const { t } = useTranslation("login")
  const navigate = useNavigate()
  const [username, setUsername] = useState("")
  const [password, setPassword] = useState("")
  const [error, setError] = useState<LoginError | "credentials" | null>(initialError)
  const [pending, setPending] = useState(false)
  // When SSO is available, the local form stays behind a link.
  const [showLocal, setShowLocal] = useState(false)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    setPending(true)
    try {
      await session.login(username, password)
      navigate(redirectTo, { replace: true })
    } catch {
      setError("credentials")
      setPending(false)
    }
  }

  const ssoLogin = async (spId: number) => {
    setError(null)
    setPending(true)
    try {
      await session.startSSOLogin(spId, redirectTo === "/" ? null : redirectTo)
    } catch (err) {
      console.error("SSO login failed:", err)
      setError("saml_auth_failed")
      setPending(false)
    }
  }

  const hasSso = ssoConfigs.length > 0

  return (
    <div className="flex min-h-svh items-center justify-center p-4">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle className="text-xl">
            <h1>{t("title")}</h1>
          </CardTitle>
          <CardDescription>{t("subtitle")}</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          {sessionExpired && !error && (
            <Alert>
              <AlertDescription>{t("sessionExpired")}</AlertDescription>
            </Alert>
          )}
          {error && (
            <Alert variant="destructive">
              <TriangleAlertIcon />
              <AlertDescription>{t(`errors.${error}`)}</AlertDescription>
            </Alert>
          )}

          {ssoConfigs.map((config) => (
            <Button key={config.sp_id} disabled={pending} onClick={() => void ssoLogin(config.sp_id)}>
              <LogInIcon data-icon="inline-start" />
              {t("ssoButton", { label: config.label })}
            </Button>
          ))}

          {hasSso && !showLocal ? (
            <Button variant="link" className="self-center" onClick={() => setShowLocal(true)}>
              {t("showLocal")}
            </Button>
          ) : (
            <form onSubmit={submit} className="contents">
              {hasSso && <FieldSeparator className="[&>span]:bg-card">{t("or")}</FieldSeparator>}
              <FieldGroup>
                <Field>
                  <FieldLabel htmlFor="login-username">{t("username")}</FieldLabel>
                  <Input
                    id="login-username"
                    autoComplete="username"
                    required
                    autoFocus
                    value={username}
                    onChange={(e) => setUsername(e.target.value)}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="login-password">{t("password")}</FieldLabel>
                  <Input
                    id="login-password"
                    type="password"
                    autoComplete="current-password"
                    required
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                  />
                </Field>
                <Button type="submit" disabled={pending || !username || !password}>
                  {pending && <Loader2Icon data-icon="inline-start" className="animate-spin" />}
                  {t("submit")}
                </Button>
              </FieldGroup>
            </form>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
