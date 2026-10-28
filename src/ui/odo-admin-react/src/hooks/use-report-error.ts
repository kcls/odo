import { toast } from "sonner"

import { describeError } from "@/api/errors"

/**
 * Surface a caught error as a toast with a friendly message, and log it
 * for diagnostics. For errors a component doesn't handle inline.
 */
export function useReportError() {
  return (err: unknown, context: string) => {
    console.error(context, err)
    toast.error(describeError(err))
  }
}
