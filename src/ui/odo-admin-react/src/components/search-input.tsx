import { SearchIcon } from "lucide-react"
import { useEffect, useState } from "react"

import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group"
import { cn } from "@/lib/utils"

/**
 * A search box that reports its value after the user pauses typing.
 * `value` is the committed value (usually from the URL); while a commit
 * is in flight the user's newer typing wins over the older value coming
 * back, and any other change to `value` (back/forward) replaces the text.
 */
export function SearchInput({
  value,
  onCommit,
  label,
  placeholder,
  className,
  delay = 300,
}: {
  value: string
  onCommit: (value: string) => void
  label: string
  placeholder?: string
  className?: string
  delay?: number
}) {
  const [text, setText] = useState(value)
  // The last value this input committed, or that arrived from outside.
  const [synced, setSynced] = useState(value)
  if (value !== synced) {
    setSynced(value)
    setText(value)
  }

  useEffect(() => {
    if (text === synced) return
    const timer = setTimeout(() => {
      setSynced(text)
      onCommit(text)
    }, delay)
    return () => clearTimeout(timer)
  }, [text, synced, onCommit, delay])

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
        onChange={(e) => setText(e.target.value)}
      />
    </InputGroup>
  )
}
