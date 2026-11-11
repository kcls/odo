import { describe, expect, it } from "vitest"

import { EMAIL_FRAME_CSP, withEmailFrameCsp } from "./email-frame"

describe("withEmailFrameCsp", () => {
  it("puts the policy ahead of the email", () => {
    expect(withEmailFrameCsp("<!DOCTYPE html><p>Hi</p>")).toBe(
      `<meta http-equiv="Content-Security-Policy" content="${EMAIL_FRAME_CSP}"><!DOCTYPE html><p>Hi</p>`,
    )
  })

  it("allows no remote loads", () => {
    expect(EMAIL_FRAME_CSP).toMatch(/^default-src 'none';/)
    expect(EMAIL_FRAME_CSP).not.toMatch(/https?:|\*|'self'/)
  })
})
