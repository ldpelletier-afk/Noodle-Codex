import { useEffect, useState } from 'react'
import {
  Sun,
  Moon,
  MoonStar,
  Coffee,
  Flame,
  Check,
  FolderOpen,
  FolderPlus,
  Trash2,
  BookText,
  RefreshCw,
  type LucideIcon,
} from 'lucide-react'
import { AppShell } from '@/components/AppShell'
import { THEMES, applyTheme, getStoredTheme, type ThemeId } from '@/lib/theme'
import { useLibrary } from '@/contexts/LibraryContext'
import { useToast } from '@/contexts/ToastContext'
import {
  attachBibtex,
  detachBibtex,
  getBibtexStatus,
  pickBibtexFile,
  rematchBibtex,
} from '@/lib/api'
import type { BibtexStatus } from '@/lib/types'
import { cn } from '@/lib/utils'

const THEME_ICONS: Record<string, LucideIcon> = {
  light: Sun,
  dark: Moon,
  black: MoonStar,
  amber: Coffee,
  ember: Flame,
}

function Section({ title, description, children }: { title: string; description?: string; children: React.ReactNode }) {
  return (
    <section className="mb-8">
      <h2 className="font-display text-base">{title}</h2>
      {description && <p className="text-sm text-muted-foreground mt-0.5 mb-4">{description}</p>}
      {!description && <div className="mb-4" />}
      {children}
    </section>
  )
}

export function SettingsPage() {
  const [theme, setTheme] = useState<ThemeId>(() => getStoredTheme())
  const { folders, scanning, addFolder, removeFolder, refresh } = useLibrary()
  const { toast } = useToast()
  const [bibtex, setBibtex] = useState<BibtexStatus | null>(null)
  const [bibBusy, setBibBusy] = useState(false)

  useEffect(() => {
    getBibtexStatus()
      .then(setBibtex)
      .catch(() => {})
  }, [])

  function selectTheme(id: ThemeId) {
    applyTheme(id)
    setTheme(id)
  }

  async function handleAttachBib() {
    const path = await pickBibtexFile()
    if (!path) return
    setBibBusy(true)
    try {
      const status = await attachBibtex(path)
      setBibtex(status)
      await refresh()
      toast.success(`Matched ${status.matched} of ${status.total} documents`)
    } catch (e) {
      toast.error(`Couldn't attach BibTeX: ${e}`)
    } finally {
      setBibBusy(false)
    }
  }

  async function handleDetachBib() {
    setBibBusy(true)
    try {
      const status = await detachBibtex()
      setBibtex(status)
      await refresh()
      toast.info('Detached BibTeX file')
    } catch (e) {
      toast.error(`Couldn't detach: ${e}`)
    } finally {
      setBibBusy(false)
    }
  }

  async function handleRematch() {
    setBibBusy(true)
    try {
      const status = await rematchBibtex()
      setBibtex(status)
      await refresh()
      toast.success(`Matched ${status.matched} of ${status.total} documents`)
    } catch (e) {
      toast.error(`Couldn't re-match: ${e}`)
    } finally {
      setBibBusy(false)
    }
  }

  async function handleAdd() {
    try {
      const result = await addFolder()
      if (!result) return
      if (result.indexed === 0) {
        toast.warning('No PDFs found. If this folder is under Desktop/Documents, grant Codex file access in System Settings → Privacy.')
      } else {
        toast.success(`Indexing ${result.indexed} PDF${result.indexed === 1 ? '' : 's'}…`)
      }
    } catch (e) {
      toast.error(`Couldn't scan folder: ${e}`)
    }
  }

  async function handleRemove(path: string, name: string) {
    try {
      await removeFolder(path)
      toast.info(`Removed "${name}" from the library`)
    } catch (e) {
      toast.error(`Couldn't remove folder: ${e}`)
    }
  }

  return (
    <AppShell>
      <div className="max-w-2xl mx-auto px-5 md:px-8 py-6">
        <h1 className="font-display text-2xl mb-6">Settings</h1>

        <Section
          title="Collections"
          description="Folders Codex scans for PDFs. Each folder becomes a collection; its files are indexed and their first pages rendered as covers."
        >
          <div className="flex flex-col gap-2">
            {folders.length === 0 && (
              <p className="text-sm text-muted-foreground">No folders added yet.</p>
            )}
            {folders.map(f => (
              <div
                key={f.path}
                className="flex items-center gap-3 p-3 rounded-xl border border-border bg-card"
              >
                <FolderOpen className="w-4 h-4 text-muted-foreground shrink-0" />
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium truncate">{f.name}</p>
                  <p className="text-xs text-muted-foreground truncate">{f.path}</p>
                </div>
                <span className="text-xs text-muted-foreground shrink-0">
                  {f.documentCount} doc{f.documentCount === 1 ? '' : 's'}
                </span>
                <button
                  onClick={() => handleRemove(f.path, f.name)}
                  title="Remove collection"
                  className="w-8 h-8 flex items-center justify-center rounded-lg text-muted-foreground hover:text-destructive hover:bg-muted transition-colors shrink-0"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </div>
            ))}
          </div>
          <button
            onClick={handleAdd}
            disabled={scanning}
            className="flex items-center gap-2 mt-3 px-3.5 py-2 rounded-lg bg-primary text-primary-foreground text-sm font-medium hover:opacity-90 transition-opacity disabled:opacity-50"
          >
            <FolderPlus className="w-4 h-4" />
            Add folder
          </button>
        </Section>

        <Section
          title="BibTeX & Zotero"
          description="Attach a central .bib file (e.g. a Zotero / Better BibTeX export). Codex matches it to your PDFs by DOI, file name, then title, and uses its metadata in place of the PDF's. Any .bib inside a collection folder is also picked up automatically."
        >
          {bibtex?.path ? (
            <div className="flex items-center gap-3 p-3 rounded-xl border border-border bg-card">
              <BookText className="w-4 h-4 text-primary shrink-0" />
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium truncate">{bibtex.path.split('/').pop()}</p>
                <p className="text-xs text-muted-foreground truncate">{bibtex.path}</p>
              </div>
              <span className="text-xs text-muted-foreground shrink-0">
                {bibtex.matched}/{bibtex.total} matched
              </span>
              <button
                onClick={handleDetachBib}
                disabled={bibBusy}
                title="Detach"
                className="w-8 h-8 flex items-center justify-center rounded-lg text-muted-foreground hover:text-destructive hover:bg-muted transition-colors shrink-0 disabled:opacity-50"
              >
                <Trash2 className="w-4 h-4" />
              </button>
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">No central BibTeX file attached.</p>
          )}
          <div className="flex items-center gap-2 mt-3">
            <button
              onClick={handleAttachBib}
              disabled={bibBusy}
              className="flex items-center gap-2 px-3.5 py-2 rounded-lg bg-primary text-primary-foreground text-sm font-medium hover:opacity-90 transition-opacity disabled:opacity-50"
            >
              <BookText className="w-4 h-4" />
              {bibtex?.path ? 'Replace file' : 'Attach .bib file'}
            </button>
            <button
              onClick={handleRematch}
              disabled={bibBusy}
              className="flex items-center gap-2 px-3.5 py-2 rounded-lg border border-border text-sm hover:bg-muted transition-colors disabled:opacity-50"
            >
              <RefreshCw className={cn('w-4 h-4', bibBusy && 'animate-spin')} />
              Re-match
            </button>
          </div>
        </Section>

        <Section title="Appearance" description="Pick a theme for the library.">
          <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
            {THEMES.map(t => {
              const Icon = THEME_ICONS[t.id] ?? Sun
              const active = theme === t.id
              return (
                <button
                  key={t.id}
                  onClick={() => selectTheme(t.id)}
                  className={cn(
                    'flex items-center gap-2 p-3 rounded-xl border transition-colors text-left',
                    active ? 'border-primary ring-1 ring-primary bg-primary/5' : 'border-border hover:bg-muted'
                  )}
                >
                  <span
                    className="w-6 h-6 rounded-full border border-border shrink-0"
                    style={{ background: t.preview.primary }}
                  />
                  <Icon className="w-4 h-4 shrink-0 text-muted-foreground" />
                  <span className="flex-1 text-sm font-medium">{t.label}</span>
                  {active && <Check className="w-4 h-4 text-primary shrink-0" />}
                </button>
              )
            })}
          </div>
        </Section>

        <Section title="Reading tracking">
          <p className="text-sm text-muted-foreground">
            In-app reading and annotation (Skim-style) is planned for a later phase. For now,
            “Open PDF” opens the file in Skim if it's installed, falling back to your system
            default viewer otherwise.
          </p>
        </Section>
      </div>
    </AppShell>
  )
}
