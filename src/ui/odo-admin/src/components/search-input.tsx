import { SearchIcon } from "lucide-react"
import { useEffect, useId, useState } from "react"
import { useTranslation } from "react-i18next"

import { InputGroup, InputGroupAddon, InputGroupInput, InputGroupText } from "@/components/ui/input-group"
import { MIN_SEARCH_LENGTH } from "@/lib/list-state"
import { cn } from "@/lib/utils"

/**
 * A search box that reports its value after the user pauses typing.
 * `value` is the committed value (usually from the URL); while a commit
 * is in flight the user's newer typing wins over the older value coming
 * back, and any other change to `value` (back/forward) replaces the text.
 *
 * Text shorter than `minLength` commits as an empty search, with a hint
 * to keep typing, so a server search never runs on one or two letters.
 * Client-side filters can pass `minLength={1}`.
 */
export function SearchInput({
  value,
  onCommit,
  label,
  placeholder,
  className,
  delay = 300,
  minLength = MIN_SEARCH_LENGTH,
}: {
  value: string
  onCommit: (value: string) => void
  label: string
  placeholder?: string
  className?: string
  delay?: number
  minLength?: number
}) {
  const { t } = useTranslation("common")
  const hintId = useId()
  const [text, setText] = useState(value)
  // The last value this input committed, or that arrived from outside.
  const [synced, setSynced] = useState(value)
  if (value !== synced) {
    setSynced(value)
    setText(value)
  }

  const tooShort = text.trim() !== "" && text.trim().length < minLength
  const effective = tooShort ? "" : text

  useEffect(() => {
    if (effective === synced) return
    const timer = setTimeout(() => {
      setSynced(effective)
      onCommit(effective)
    }, delay)
    return () => clearTimeout(timer)
  }, [effective, synced, onCommit, delay])

  return (
    <InputGroup className={cn("max-w-xs", className)}>
      <InputGroupAddon>
        <SearchIcon />
      </InputGroupAddon>
      <InputGroupInput
        type="search"
        value={text}
        placeholder={placeholder}
        aria-label={label}
        aria-describedby={hintId}
        onChange={(e) => setText(e.target.value)}
      />
      <InputGroupAddon align="inline-end">
        <InputGroupText id={hintId} aria-live="polite" className="text-xs">
          {tooShort && t("search.minLength", { count: minLength })}
        </InputGroupText>
      </InputGroupAddon>
    </InputGroup>
  )
}
