import { NavLink } from 'react-router-dom'
import { Library, BookOpen, BarChart3, Settings, X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { useLibrary } from '@/contexts/LibraryContext'
import { CodexMark } from '@/components/CodexMark'
import { ThemeToggle } from '@/components/ThemeToggle'

const NAV_ITEMS = [
  { to: '/', label: 'Library', icon: Library, end: true, tour: 'nav-library' },
  { to: '/reading', label: 'Currently reading', icon: BookOpen, end: false, tour: 'nav-reading' },
  { to: '/stats', label: 'Insights', icon: BarChart3, end: false },
  { to: '/settings', label: 'Settings', icon: Settings, end: false },
]

function NavList({ onNavigate }: { onNavigate?: () => void }) {
  const { documents } = useLibrary()
  const readingCount = documents.filter(d => d.status === 'in_progress').length

  return (
    <nav className="flex flex-col gap-0.5 px-2">
      {NAV_ITEMS.map(({ to, label, icon: Icon, end, tour }) => {
        const badge = to === '/reading' && readingCount > 0 ? readingCount : null
        return (
          <NavLink
            key={to}
            to={to}
            end={end}
            onClick={onNavigate}
            data-tour={tour}
            className={({ isActive }) =>
              cn(
                'flex items-center gap-2.5 px-2.5 py-2 rounded-lg text-sm font-medium transition-colors touch-feedback',
                isActive
                  ? 'bg-primary/10 text-primary'
                  : 'text-muted-foreground hover:text-foreground hover:bg-muted'
              )
            }
          >
            <Icon className="w-4 h-4 shrink-0" />
            <span className="truncate">{label}</span>
            {badge !== null && (
              <span className="ml-auto shrink-0 tabular-nums text-xs opacity-70">{badge}</span>
            )}
          </NavLink>
        )
      })}
    </nav>
  )
}

export function Sidebar({
  mobileOpen,
  onMobileClose,
}: {
  mobileOpen: boolean
  onMobileClose: () => void
}) {
  return (
    <>
      {/* Desktop — docked */}
      <aside className="hidden md:flex w-56 shrink-0 flex-col border-r border-border bg-card/40 py-4">
        <div className="px-4 mb-6">
          <CodexMark />
        </div>
        <div className="flex-1">
          <NavList />
        </div>
        <div className="px-3 mt-4">
          <ThemeToggle />
        </div>
      </aside>

      {/* Mobile — drawer */}
      {mobileOpen && (
        <div className="md:hidden fixed inset-0 z-50 flex">
          <div className="absolute inset-0 bg-black/40" onClick={onMobileClose} />
          <aside className="relative w-64 h-full bg-card border-r border-border flex flex-col py-4 animate-in slide-in-from-left">
            <div className="px-4 mb-6 flex items-center justify-between">
              <CodexMark />
              <button
                onClick={onMobileClose}
                className="w-8 h-8 flex items-center justify-center rounded-lg text-muted-foreground hover:bg-muted"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
            <div className="flex-1">
              <NavList onNavigate={onMobileClose} />
            </div>
            <div className="px-3 mt-4">
              <ThemeToggle />
            </div>
          </aside>
        </div>
      )}
    </>
  )
}
