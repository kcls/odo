import { Loader2Icon, RefreshCwIcon, Trash2Icon, TriangleAlertIcon } from "lucide-react"
import { useEffect, useRef, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { data, useBlocker, useLoaderData, useNavigate, type LoaderFunctionArgs } from "react-router"
import { toast } from "sonner"

import { ApiRequestError } from "@/api/client"
import { describeError } from "@/api/errors"
import { templatesApi, type TemplateRow } from "@/api/notify"
import { PERMS } from "@/auth/perms"
import { checkPerms } from "@/auth/session"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { DetailHeader } from "@/components/detail-header"
import { StatusBadge } from "@/components/status-badge"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Field, FieldContent, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { loadNamespaces } from "@/i18n"
import { requireIdParam } from "@/lib/loader"
import type { RouteHandle } from "@/lib/route-handle"
import { formatJsonText, parseJsonText } from "@/lib/json-text"
import { useTemplatePreview } from "./use-template-preview"

export async function loader({ params }: LoaderFunctionArgs) {
  // The "new" route has no :templateId.
  const id = params.templateId === undefined ? null : requireIdParam(params.templateId)
  const [template, perms] = await Promise.all([
    id === null ? null : templatesApi.find(id),
    checkPerms({ canWrite: PERMS.TEMPLATE_WRITE }),
    loadNamespaces("templates"),
  ])
  if (template === undefined) throw data("Template not found", { status: 404 })
  return { template, ...perms }
}

export const handle: RouteHandle = {
  crumb: (match, t) =>
    (match.loaderData as Awaited<ReturnType<typeof loader>> | undefined)?.template?.name ??
    t("editor.newTitle", { ns: "templates" }),
}

export function Component() {
  const { template, canWrite } = useLoaderData<typeof loader>()
  // A fresh editor per template, so switching between them resets the form.
  return <Editor key={template?.id ?? "new"} template={template} canWrite={canWrite} />
}

type Draft = {
  code: string
  name: string
  description: string
  isActive: boolean
  subject: string
  body: string
  bodyHtml: string
  sampleData: string
}

function draftOf(template: TemplateRow | null): Draft {
  return {
    code: template?.code ?? "",
    name: template?.name ?? "",
    description: template?.description ?? "",
    isActive: template?.is_active ?? true,
    subject: template?.subject_template ?? "",
    body: template?.body_template ?? "",
    bodyHtml: template?.body_template_html ?? "",
    sampleData: formatJsonText(template?.sample_data),
  }
}

const sameDraft = (a: Draft, b: Draft) => (Object.keys(a) as (keyof Draft)[]).every((k) => a[k] === b[k])

/**
 * Edit form beside a live preview of the template as it stands, sample
 * data included. Leaving with unsaved edits asks first.
 */
function Editor({ template, canWrite }: { template: TemplateRow | null; canWrite: boolean }) {
  const { t } = useTranslation(["templates", "common"])
  const navigate = useNavigate()
  const [saved, setSaved] = useState(() => draftOf(template))
  const [draft, setDraft] = useState(saved)
  const [codeError, setCodeError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)
  const [deleting, setDeleting] = useState(false)
  // Set just before navigating away on purpose (after a save or delete),
  // when the dirty state from this render is stale.
  const leaving = useRef(false)

  const dirty = canWrite && !sameDraft(draft, saved)
  const set = <K extends keyof Draft>(key: K) => (value: Draft[K]) => setDraft((d) => ({ ...d, [key]: value }))
  const sampleOk = parseJsonText(draft.sampleData).ok
  const canSave =
    canWrite && !pending && draft.code.trim() && draft.name.trim() && draft.subject.trim() && draft.body.trim()

  const preview = useTemplatePreview(draft)

  const blocker = useBlocker(
    ({ currentLocation, nextLocation }) =>
      dirty && !leaving.current && currentLocation.pathname !== nextLocation.pathname
  )
  useEffect(() => {
    if (!dirty) return
    // Reloads and closing the tab, which the router can't intercept.
    const warn = (e: BeforeUnloadEvent) => e.preventDefault()
    window.addEventListener("beforeunload", warn)
    return () => window.removeEventListener("beforeunload", warn)
  }, [dirty])

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    const sample = parseJsonText(draft.sampleData)
    if (!sample.ok || !canSave) return
    setPending(true)
    setCodeError(null)
    setError(null)
    const fields = {
      code: draft.code.trim(),
      name: draft.name.trim(),
      description: draft.description.trim(),
      is_active: draft.isActive,
      subject_template: draft.subject,
      body_template: draft.body,
      body_template_html: draft.bodyHtml,
      ...(sample.value !== undefined ? { sample_data: sample.value } : {}),
    }
    try {
      if (template) {
        const updated = await templatesApi.update(template.id, fields)
        setSaved(draft)
        toast.success(t("editor.saved", { name: updated.name }))
      } else {
        const created = await templatesApi.create(fields)
        toast.success(t("editor.created", { name: created.name }))
        leaving.current = true
        navigate(`../${created.id}`, { replace: true })
      }
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === "TEMPLATE_CODE_TAKEN") {
        setCodeError(t("common:errors.codes.TEMPLATE_CODE_TAKEN"))
      } else {
        setError(describeError(err))
      }
    } finally {
      setPending(false)
    }
  }

  const deleteTemplate = async () => {
    if (!template) return
    try {
      await templatesApi.delete(template.id)
      toast.success(t("delete.done", { name: template.name }))
      leaving.current = true
      navigate("..", { replace: true })
    } catch (err) {
      toast.error(describeError(err))
    }
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-6">
      <DetailHeader
        backTo=".."
        backLabel={t("editor.back")}
        title={template ? saved.name || template.code : t("editor.newTitle")}
        summary={
          template
            ? [
                <span className="font-mono text-xs">{saved.code}</span>,
                saved.isActive ? (
                  <StatusBadge tone="success">{t("active")}</StatusBadge>
                ) : (
                  <StatusBadge tone="neutral">{t("inactive")}</StatusBadge>
                ),
              ]
            : undefined
        }
        actions={
          canWrite && (
            <>
              {dirty && <StatusBadge tone="warning">{t("editor.unsaved")}</StatusBadge>}
              {template && (
                <Button type="button" variant="destructive" onClick={() => setDeleting(true)}>
                  <Trash2Icon data-icon="inline-start" />
                  {t("editor.delete")}
                </Button>
              )}
              <Button type="submit" disabled={!canSave || !sampleOk || (template !== null && !dirty)}>
                {pending && <Loader2Icon data-icon="inline-start" className="animate-spin" />}
                {t("editor.save")}
              </Button>
            </>
          )
        }
      />

      {!canWrite && (
        <Alert>
          <AlertDescription>{t("editor.readOnly")}</AlertDescription>
        </Alert>
      )}

      <div className="grid items-start gap-6 lg:grid-cols-2">
        <Card>
          <CardContent>
            <fieldset disabled={!canWrite} className="contents">
              <FieldGroup>
                <div className="grid gap-4 sm:grid-cols-2">
                  <Field data-invalid={codeError !== null}>
                    <FieldLabel htmlFor="template-code">{t("editor.code")}</FieldLabel>
                    <Input
                      id="template-code"
                      className="font-mono"
                      required
                      autoFocus={!template}
                      value={draft.code}
                      placeholder={t("editor.codePlaceholder")}
                      aria-invalid={codeError !== null}
                      onChange={(e) => set("code")(e.target.value)}
                    />
                    {codeError && <FieldError>{codeError}</FieldError>}
                  </Field>
                  <Field>
                    <FieldLabel htmlFor="template-name">{t("editor.name")}</FieldLabel>
                    <Input
                      id="template-name"
                      required
                      value={draft.name}
                      onChange={(e) => set("name")(e.target.value)}
                    />
                  </Field>
                </div>
                <Field>
                  <FieldLabel htmlFor="template-description">{t("editor.description")}</FieldLabel>
                  <Input
                    id="template-description"
                    value={draft.description}
                    onChange={(e) => set("description")(e.target.value)}
                  />
                </Field>
                <Field orientation="horizontal">
                  <Checkbox
                    id="template-active"
                    checked={draft.isActive}
                    onCheckedChange={(v) => set("isActive")(v === true)}
                  />
                  <FieldContent>
                    <FieldLabel htmlFor="template-active" className="font-normal">
                      {t("editor.isActive")}
                    </FieldLabel>
                    <FieldDescription>{t("editor.isActiveHint")}</FieldDescription>
                  </FieldContent>
                </Field>
                <Field>
                  <FieldLabel htmlFor="template-subject">{t("editor.subject")}</FieldLabel>
                  <Input
                    id="template-subject"
                    className="font-mono text-xs"
                    required
                    value={draft.subject}
                    onChange={(e) => set("subject")(e.target.value)}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="template-body">{t("editor.body")}</FieldLabel>
                  <Textarea
                    id="template-body"
                    className="min-h-28 font-mono text-xs"
                    required
                    value={draft.body}
                    onChange={(e) => set("body")(e.target.value)}
                  />
                  <FieldDescription>{t("editor.bodyHint")}</FieldDescription>
                </Field>
                <Field>
                  <FieldLabel htmlFor="template-body-html">{t("editor.bodyHtml")}</FieldLabel>
                  <Textarea
                    id="template-body-html"
                    className="min-h-64 font-mono text-xs"
                    value={draft.bodyHtml}
                    onChange={(e) => set("bodyHtml")(e.target.value)}
                  />
                  <FieldDescription>{t("editor.bodyHtmlHint")}</FieldDescription>
                </Field>
                <Field data-invalid={!sampleOk}>
                  <FieldLabel htmlFor="template-sample">{t("editor.sampleData")}</FieldLabel>
                  <Textarea
                    id="template-sample"
                    className="min-h-32 font-mono text-xs"
                    value={draft.sampleData}
                    aria-invalid={!sampleOk}
                    onChange={(e) => set("sampleData")(e.target.value)}
                  />
                  {sampleOk ? (
                    <FieldDescription>{t("editor.sampleDataHint")}</FieldDescription>
                  ) : (
                    <FieldError>{t("editor.invalidJson")}</FieldError>
                  )}
                </Field>
                {error && <FieldError>{error}</FieldError>}
              </FieldGroup>
            </fieldset>
          </CardContent>
        </Card>

        <PreviewCard preview={preview} />
      </div>

      <ConfirmDialog
        open={deleting}
        onOpenChange={setDeleting}
        title={t("delete.title")}
        description={t("delete.body", { name: saved.name })}
        confirmLabel={t("common:actions.delete")}
        destructive
        onConfirm={deleteTemplate}
      />
      <ConfirmDialog
        open={blocker.state === "blocked"}
        onOpenChange={(open) => !open && blocker.state === "blocked" && blocker.reset()}
        title={t("leave.title")}
        description={t("leave.body")}
        confirmLabel={t("leave.discard")}
        cancelLabel={t("leave.stay")}
        destructive
        onConfirm={() => {
          if (blocker.state === "blocked") blocker.proceed()
        }}
      />
    </form>
  )
}

function PreviewCard({ preview }: { preview: ReturnType<typeof useTemplatePreview> }) {
  const { t } = useTranslation("templates")
  const p = preview.preview
  return (
    <Card className="lg:sticky lg:top-4">
      <CardHeader>
        <CardTitle>
          <h2>{t("preview.title")}</h2>
        </CardTitle>
        <CardDescription>{t("preview.description")}</CardDescription>
        <CardAction>
          <Button type="button" variant="ghost" size="sm" onClick={preview.refresh} disabled={preview.rendering}>
            {preview.rendering ? (
              <Loader2Icon data-icon="inline-start" className="animate-spin" />
            ) : (
              <RefreshCwIcon data-icon="inline-start" />
            )}
            {preview.rendering ? t("preview.rendering") : t("preview.refresh")}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4" aria-live="polite" aria-busy={preview.rendering}>
        {preview.error ? (
          <Alert variant="destructive">
            <TriangleAlertIcon />
            <AlertDescription>
              {preview.error.kind === "json" ? (
                t("editor.invalidJson")
              ) : (
                // Handlebars syntax errors are multi-line, with a caret
                // pointing at the problem; keep their layout.
                <pre className="font-mono text-xs whitespace-pre-wrap">
                  {t("preview.failed", { message: preview.error.message })}
                </pre>
              )}
            </AlertDescription>
          </Alert>
        ) : !p || (p.subject == null && p.body == null && p.body_html == null) ? (
          <p className="text-sm text-muted-foreground">{preview.rendering ? t("preview.rendering") : t("preview.nothing")}</p>
        ) : (
          <>
            {p.subject != null && (
              <PreviewPart label={t("preview.subject")}>
                <p className="font-medium">{p.subject}</p>
              </PreviewPart>
            )}
            {p.body != null && (
              <PreviewPart label={t("preview.body")}>
                <pre className="font-sans text-sm whitespace-pre-wrap">{p.body}</pre>
              </PreviewPart>
            )}
            {p.body_html != null && (
              <PreviewPart label={t("preview.bodyHtml")}>
                {/* An empty sandbox: no scripts, forms, or same-origin access,
                    and the email's own styles stay inside the frame. Email
                    is designed for a light background, as clients show it. */}
                <iframe
                  title={t("preview.htmlFrame")}
                  sandbox=""
                  srcDoc={p.body_html}
                  className="h-96 w-full rounded-md border bg-white"
                />
              </PreviewPart>
            )}
          </>
        )}
      </CardContent>
    </Card>
  )
}

function PreviewPart({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <h3 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">{label}</h3>
      <div className="rounded-md border bg-muted/30 p-3">{children}</div>
    </div>
  )
}
