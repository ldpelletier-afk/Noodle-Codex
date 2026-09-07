import { useEffect, useState } from 'react'
import { HashRouter, Routes, Route, Navigate } from 'react-router-dom'
import { Keyboard } from 'lucide-react'
import { ToastProvider } from '@/contexts/ToastContext'
import { LibrarySearchProvider } from '@/contexts/LibrarySearchContext'
import { LibraryProvider } from '@/contexts/LibraryContext'
import { KeyboardShortcutsModal } from '@/components/KeyboardShortcutsModal'
import { LibraryPage } from '@/pages/LibraryPage'
import { CurrentlyReadingPage } from '@/pages/CurrentlyReadingPage'
import { DocumentDetailPage } from '@/pages/DocumentDetailPage'
import { StatsPage } from '@/pages/StatsPage'
import { SettingsPage } from '@/pages/SettingsPage'
import { applyTheme, getStoredTheme } from '@/lib/theme'

// Apply the saved theme immediately, before first paint, to avoid a flash.
applyTheme(getStoredTheme())

function AppRoutes() {
  const [shortcutsOpen, setShortcutsOpen] = useState(false)

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const tag = (e.target as HTMLElement).tagName
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return
      if (e.key === '?') {
        e.preventDefault()
        setShortcutsOpen(s => !s)
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])

  return (
    <>
      <Routes>
        <Route path="/" element={<LibraryPage />} />
        <Route path="/reading" element={<CurrentlyReadingPage />} />
        <Route path="/documents/:id" element={<DocumentDetailPage />} />
        <Route path="/stats" element={<StatsPage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>

      <button
        onClick={() => setShortcutsOpen(s => !s)}
        title="Keyboard shortcuts (?)"
        className="fixed bottom-5 right-5 z-40 w-9 h-9 rounded-full bg-card border border-border shadow-md flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-muted transition-all"
      >
        <Keyboard className="w-4 h-4" />
      </button>

      <KeyboardShortcutsModal open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />
    </>
  )
}

function App() {
  return (
    <HashRouter>
      <ToastProvider>
        <LibraryProvider>
          <LibrarySearchProvider>
            <AppRoutes />
          </LibrarySearchProvider>
        </LibraryProvider>
      </ToastProvider>
    </HashRouter>
  )
}

export default App
