import { ChevronDownIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"

import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { cn } from "@/lib/utils"

export type ComboboxOption = {
  value: string
  label: string
  /** Secondary text, e.g. a code; also matched by the search. */
  detail?: string
  /** Tree depth, for indenting hierarchical options. */
  depth?: number
}

/**
 * A searchable single-select for long option lists (roles, permissions,
 * org units) where a plain Select would mean scrolling through hundreds.
 */
export function Combobox({
  id,
  options,
  value,
  onValueChange,
  placeholder,
  searchPlaceholder,
  disabled = false,
  invalid = false,
}: {
  id?: string
  options: readonly ComboboxOption[]
  value: string
  onValueChange: (value: string) => void
  placeholder: string
  searchPlaceholder: string
  disabled?: boolean
  invalid?: boolean
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const selected = options.find((o) => o.value === value)

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          id={id}
          type="button"
          role="combobox"
          aria-expanded={open}
          aria-invalid={invalid || undefined}
          disabled={disabled}
          className="flex h-9 w-full items-center justify-between gap-2 rounded-md border border-input bg-transparent px-2.5 text-start text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive dark:bg-input/30"
        >
          <span className={cn("truncate", !selected && "text-muted-foreground")}>
            {selected ? selected.label : placeholder}
          </span>
          <ChevronDownIcon className="size-4 shrink-0 text-muted-foreground" />
        </button>
      </PopoverTrigger>
      <PopoverContent className="w-(--radix-popover-trigger-width) p-0" align="start">
        <Command label={searchPlaceholder}>
          <CommandInput placeholder={searchPlaceholder} />
          <CommandList className="max-h-64">
            <CommandEmpty>{t("combobox.noMatches")}</CommandEmpty>
            <CommandGroup>
              {options.map((o) => (
                <CommandItem
                  key={o.value}
                  // cmdk matches on this; ids keep identical labels distinct.
                  value={`${o.label} ${o.detail ?? ""} ${o.value}`}
                  data-checked={o.value === value}
                  onSelect={() => {
                    onValueChange(o.value)
                    setOpen(false)
                  }}
                >
                  <span
                    className="flex min-w-0 flex-1 flex-col"
                    style={o.depth ? { paddingInlineStart: `${o.depth}rem` } : undefined}
                  >
                    <span className="truncate">{o.label}</span>
                    {o.detail && (
                      <span className="truncate font-mono text-xs text-muted-foreground">{o.detail}</span>
                    )}
                  </span>
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}
