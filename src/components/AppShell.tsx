import { useEffect, useRef, useState, type ReactNode } from 'react'
import { useNavigate, useLocation } from 'react-router-dom'
import { Sidebar } from '@/components/Sidebar'
import { AppHeader, HeaderSearch } from '@/components/AppHeader'
import { useLibrarySearch } from '@/contexts/LibrarySearchContext'

export function AppShell({ children, actions }: { children: ReactNode; actions?: ReactNode }) {
  const navigate = useNavigate()
  const location = useLocation()
  const { query, setQuery } = useLibrarySearch()
  const [mobileSidebarOpen, setMobileSidebarOpen] = useState(false)
  const searchInputRef = useRef<HTMLInputElement>(null)

  function submitSearch(e: React.FormEvent) {
    e.preventDefault()
    if (location.pathname !== '/') navigate('/')
    searchInputRef.current?.blur()
  }

  // "/" focuses the search box from anywhere in the app.
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const tag = (e.target as HTMLElement).tagName
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return
      if (e.key === '/' && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault()
        if (location.pathname !== '/') navigate('/')
        searchInputRef.current?.focus()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [location.pathname, navigate])

  return (
    <div className="h-screen bg-background flex flex-col overflow-hidden">
      <AppHeader
        onMenuClick={() => setMobileSidebarOpen(true)}
        search={
          <HeaderSearch
            value={query}
            onChange={setQuery}
            onClear={() => setQuery('')}
            onSubmit={submitSearch}
            inputRef={searchInputRef}
          />
        }
        actions={actions}
      />
      <div className="flex flex-1 overflow-hidden">
        <Sidebar mobileOpen={mobileSidebarOpen} onMobileClose={() => setMobileSidebarOpen(false)} />
        <main className="flex-1 overflow-y-auto min-w-0">{children}</main>
      </div>
    </div>
  )
}
