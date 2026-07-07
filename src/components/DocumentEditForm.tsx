import { useState } from 'react'
import { AlertTriangle, Loader2 } from 'lucide-react'
import type { Document, UpdateDocumentInput } from '@/lib/types'

function splitList(value: string): string[] {
  return value
    .split(',')
    .map(s => s.trim())
    .filter(Boolean)
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      {children}
      {hint && <span className="text-[0.7rem] text-muted-foreground">{hint}</span>}
    </label>
  )
}

const inputClass =
  'w-full px-2.5 py-1.5 text-sm rounded-lg border border-border bg-background focus:outline-none focus:ring-1 focus:ring-ring'

export function DocumentEditForm({
  doc,
  onCancel,
  onSave,
}: {
  doc: Document
  onCancel: () => void
  onSave: (input: UpdateDocumentInput) => Promise<void>
}) {
  const [title, setTitle] = useState(doc.title)
  const [authors, setAuthors] = useState(doc.authors.join(', '))
  const [year, setYear] = useState(doc.year?.toString() ?? '')
  const [venue, setVenue] = useState(doc.venue ?? '')
  const [doi, setDoi] = useState(doc.doi ?? '')
  const [keywords, setKeywords] = useState(doc.keywords.join(', '))
  const [subject, setSubject] = useState(doc.subject ?? '')
  const [saving, setSaving] = useState(false)

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setSaving(true)
    try {
      await onSave({
        id: doc.id,
        title,
        authors: splitList(authors),
        year: year.trim() ? Number(year) : null,
        venue: venue.trim() || null,
        doi: doi.trim() || null,
        keywords: splitList(keywords),
        subject: subject.trim() || null,
      })
    } finally {
      setSaving(false)
    }
  }

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-4">
      <div className="flex items-start gap-2 p-3 rounded-lg bg-warning/10 text-xs text-foreground">
        <AlertTriangle className="w-3.5 h-3.5 text-warning shrink-0 mt-0.5" />
        <span>
          <strong>Title, authors, subject, and keywords</strong> are written into the PDF file itself, and the
          file is renamed to match the new title. <strong>Year, venue, and DOI</strong> are Codex-only fields.
        </span>
      </div>

      <Field label="Title (renames the file)">
        <input className={inputClass} value={title} onChange={e => setTitle(e.target.value)} required />
      </Field>

      <Field label="Authors" hint="Comma-separated">
        <input className={inputClass} value={authors} onChange={e => setAuthors(e.target.value)} />
      </Field>

      <div className="grid grid-cols-2 gap-3">
        <Field label="Year">
          <input
            className={inputClass}
            type="number"
            value={year}
            onChange={e => setYear(e.target.value)}
          />
        </Field>
        <Field label="DOI">
          <input className={inputClass} value={doi} onChange={e => setDoi(e.target.value)} />
        </Field>
      </div>

      <Field label="Venue" hint="Journal, publisher, or conference">
        <input className={inputClass} value={venue} onChange={e => setVenue(e.target.value)} />
      </Field>

      <Field label="Keywords" hint="Comma-separated">
        <input className={inputClass} value={keywords} onChange={e => setKeywords(e.target.value)} />
      </Field>

      <Field label="Subject">
        <textarea
          className={inputClass}
          rows={3}
          value={subject}
          onChange={e => setSubject(e.target.value)}
        />
      </Field>

      <div className="flex items-center gap-2 pt-1">
        <button
          type="submit"
          disabled={saving}
          className="flex items-center gap-1.5 px-3.5 py-2 rounded-lg bg-primary text-primary-foreground text-sm font-medium hover:opacity-90 transition-opacity disabled:opacity-50"
        >
          {saving && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
          Save changes
        </button>
        <button
          type="button"
          onClick={onCancel}
          disabled={saving}
          className="px-3.5 py-2 rounded-lg border border-border text-sm hover:bg-muted transition-colors disabled:opacity-50"
        >
          Cancel
        </button>
      </div>
    </form>
  )
}
