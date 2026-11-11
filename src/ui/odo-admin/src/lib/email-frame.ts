/**
 * What an email body may load when shown in the admin UI: inline and
 * embedded content only. Remote images and stylesheets would be fetched by
 * the admin's browser, telling the sender (a tracking pixel, say) who
 * looked and when.
 */
export const EMAIL_FRAME_CSP =
  "default-src 'none'; img-src data: cid:; style-src 'unsafe-inline'; font-src data:"

/**
 * `html` with a CSP <meta> in front, for an iframe's srcdoc. Placing it
 * ahead of the email's own doctype or <html> is safe: the parser still puts
 * it in the head, and srcdoc documents are never in quirks mode.
 */
export function withEmailFrameCsp(html: string): string {
  return `<meta http-equiv="Content-Security-Policy" content="${EMAIL_FRAME_CSP}">${html}`
}
