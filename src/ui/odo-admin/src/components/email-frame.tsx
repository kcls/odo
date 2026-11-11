import { withEmailFrameCsp } from "@/lib/email-frame"

/**
 * An HTML email body, isolated from the admin UI. The empty sandbox allows
 * no scripts, forms, popups, or same-origin access, and the email's own
 * styles stay inside the frame. Its CSP blocks remote resources (see
 * EMAIL_FRAME_CSP); the page's CSP header, which srcdoc frames inherit,
 * does too, but this holds wherever the UI is served. Email is designed
 * for a light background, as mail clients show it.
 */
export function EmailFrame({ title, html }: { title: string; html: string }) {
  return (
    <iframe
      title={title}
      sandbox=""
      referrerPolicy="no-referrer"
      srcDoc={withEmailFrameCsp(html)}
      className="h-96 w-full rounded-md border bg-white"
    />
  )
}
