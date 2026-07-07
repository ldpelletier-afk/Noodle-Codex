import { useEffect, useRef, useState } from 'react'
import { Sun, Moon, MoonStar, Coffee, Flame, Check, type LucideIcon } from 'lucide-react'
import { THEMES, applyTheme, getStoredTheme, type ThemeId } from '@/lib/theme'
import { cn } from '@/lib/utils'

const THEME_ICONS: Record<string, LucideIcon> = {
  light: Sun,
  dark: Moon,
  black: MoonStar,
  amber: Coffee,
  ember: Flame,
}

export function ThemeToggle({ collapsed = false }: { collapsed?: boolean }) {
  const [open, setOpen] = useState(false)
  const [current, setCurrent] = useState<ThemeId>(() => getStoredTheme())
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    function onDown(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', onDown)
    return () => document.removeEventListener('mousedown', onDown)
  }, [])

  function select(id: ThemeId) {
    applyTheme(id)
    setCurrent(id)
    setOpen(false)
  }

  const CurrentIcon = THEME_ICONS[current] ?? Sun

  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        onClick={() => setOpen(o => !o)}
        title="Theme"
        className={cn(
          'flex items-center gap-2 rounded-lg border border-border bg-card hover:bg-muted transition-colors text-muted-foreground hover:text-foreground',
          collapsed ? 'w-9 h-9 justify-center' : 'w-full px-3 py-2'
        )}
      >
        <CurrentIcon className="w-4 h-4 shrink-0" />
        {!collapsed && <span className="text-xs font-medium">Theme</span>}
      </button>

      {open && (
        <div
          className={cn(
            'absolute z-50 bg-card border border-border rounded-xl shadow-xl shadow-accent-soft p-1.5 w-44',
            collapsed ? 'left-full ml-2 bottom-0' : 'left-0 bottom-full mb-2'
          )}
        >
          {THEMES.map(t => {
            const Icon = THEME_ICONS[t.id] ?? Sun
            const active = current === t.id
            return (
              <button
                key={t.id}
                type="button"
                onClick={() => select(t.id)}
                className={cn(
                  'flex items-center gap-2 w-full px-2 py-1.5 rounded-lg text-sm transition-colors',
                  active ? 'bg-primary/10 text-primary' : 'hover:bg-muted text-foreground'
                )}
              >
                <span
                  className="w-3.5 h-3.5 rounded-full border border-border shrink-0"
                  style={{ background: t.preview.primary }}
                />
                <Icon className="w-3.5 h-3.5 shrink-0" />
                <span className="flex-1 text-left">{t.label}</span>
                {active && <Check className="w-3.5 h-3.5 shrink-0" />}
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}
