import { Menu, Search, X } from 'lucide-react'
import type { ReactNode, RefObject } from 'react'

export function HeaderSearch({
  value,
  onChange,
  onClear,
  onSubmit,
  inputRef,
}: {
  value: string
  onChange: (v: string) => void
  onClear: () => void
  onSubmit: (e: React.FormEvent) => void
  inputRef: RefObject<HTMLInputElement | null>
}) {
  return (
    <form onSubmit={onSubmit} className="relative w-full max-w-sm">
      <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground" />
      <input
        ref={inputRef}
        value={value}
        onChange={e => onChange(e.target.value)}
        placeholder="Search your library…  (/)"
        className="w-full pl-8 pr-8 py-1.5 text-sm rounded-lg border border-border bg-background focus:outline-none focus:ring-1 focus:ring-ring"
      />
      {value && (
        <button
          type="button"
          onClick={onClear}
          className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
        >
          <X className="w-3.5 h-3.5" />
        </button>
      )}
    </form>
  )
}

export function AppHeader({
  onMenuClick,
  search,
  actions,
}: {
  onMenuClick: () => void
  search?: ReactNode
  actions?: ReactNode
}) {
  return (
    <header className="h-14 shrink-0 flex items-center gap-3 px-3 md:px-4 border-b border-border bg-card/60 backdrop-blur">
      <button
        onClick={onMenuClick}
        className="md:hidden w-9 h-9 flex items-center justify-center rounded-lg text-muted-foreground hover:bg-muted"
      >
        <Menu className="w-4 h-4" />
      </button>
      <div className="flex-1 flex items-center">{search}</div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </header>
  )
}
