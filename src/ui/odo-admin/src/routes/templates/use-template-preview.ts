import { useEffect, useRef, useState } from "react"

import { describeError } from "@/api/errors"
import { templatesApi, type PreviewResponse } from "@/api/notify"
import { parseJsonText } from "@/lib/json-text"

export type PreviewState = {
  preview: PreviewResponse | null
  /** "json" for unparseable sample data, else the server's message. */
  error: { kind: "json" } | { kind: "api"; message: string } | null
  rendering: boolean
}

type PreviewInput = { subject: string; body: string; bodyHtml: string; sampleData: string }

/**
 * Renders the template text as the user types, after a pause. Responses
 * to superseded input are dropped. `refresh` re-renders immediately.
 */
export function useTemplatePreview({ subject, body, bodyHtml, sampleData }: PreviewInput, delay = 500) {
  const [state, setState] = useState<PreviewState>({ preview: null, error: null, rendering: true })
  const [nonce, setNonce] = useState(0)
  const lastNonce = useRef(nonce)

  useEffect(() => {
    let cancelled = false
    // A manual refresh shouldn't wait out the typing pause; edits should.
    const immediate = nonce !== lastNonce.current
    lastNonce.current = nonce
    const timer = setTimeout(
      async () => {
        const sample = parseJsonText(sampleData)
        if (!sample.ok) {
          setState({ preview: null, error: { kind: "json" }, rendering: false })
          return
        }
        setState((s) => ({ ...s, rendering: true }))
        try {
          const preview = await templatesApi.preview({
            subject_template: subject || undefined,
            body_template: body || undefined,
            body_template_html: bodyHtml || undefined,
            variables: sample.value,
          })
          if (!cancelled) setState({ preview, error: null, rendering: false })
        } catch (err) {
          if (!cancelled) {
            setState({ preview: null, error: { kind: "api", message: describeError(err) }, rendering: false })
          }
        }
      },
      immediate ? 0 : delay
    )
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [subject, body, bodyHtml, sampleData, delay, nonce])

  return { ...state, refresh: () => setNonce((n) => n + 1) }
}
