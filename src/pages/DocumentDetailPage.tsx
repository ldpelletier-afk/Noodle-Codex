import { useEffect, useState } from 'react'
import { useParams, useNavigate, Link } from 'react-router-dom'
import { ArrowLeft, ExternalLink, FolderOpen, FileText, Hash, Quote, BookText, Pencil } from 'lucide-react'
import { AppShell } from '@/components/AppShell'
import { DocumentEditForm } from '@/components/DocumentEditForm'
import { ReadingControls } from '@/components/ReadingControls'
import { getDocument, openDocument, revealInFinder, exportBibtex, updateDocument, thumbnailUrl } from '@/lib/api'
import { useToast } from '@/contexts/ToastContext'
import { useLibrary } from '@/contexts/LibraryContext'
import { formatBytes, formatDate } from '@/lib/utils'
import type { Document, UpdateDocumentInput } from '@/lib/types'

export function DocumentDetailPage() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const [doc, setDoc] = useState<Document | null | undefined>(undefined)
  const [editing, setEditing] = useState(false)
  const { toast } = useToast()
  const { refresh, patchDocument } = useLibrary()

  useEffect(() => {
    let cancelled = false
    if (!id) return
    setEditing(false)
    getDocument(id).then(d => {
      if (!cancelled) setDoc(d)
    })
    return () => {
      cancelled = true
    }
  }, [id])

  async function handleSaveEdit(input: UpdateDocumentInput) {
    try {
      const { document: updated, pdfWarning } = await updateDocument(input)
      await refresh()
      if (pdfWarning) {
        toast.warning(pdfWarning)
      } else {
        toast.success(
          updated.id !== input.id
            ? `Saved — file renamed to "${updated.fileName}"`
            : 'Saved'
        )
      }
      // The id is derived from the file path, so a rename means a new id.
      navigate(`/documents/${updated.id}`, { replace: true })
    } catch (e) {
      toast.error(`Couldn't save: ${e}`)
    }
  }

  if (doc === undefined) {
    return (
      <AppShell>
        <div className="max-w-4xl mx-auto px-5 md:px-8 py-6 text-sm text-muted-foreground">Loading…</div>
      </AppShell>
    )
  }

  if (doc === null) {
    return (
      <AppShell>
        <div className="max-w-4xl mx-auto px-5 md:px-8 py-10 text-center">
          <p className="text-muted-foreground text-sm mb-3">This document couldn't be found.</p>
          <Link to="/" className="text-sm text-primary hover:underline">
            Back to library
          </Link>
        </div>
      </AppShell>
    )
  }

  const thumb = thumbnailUrl(doc.thumbnailPath)

  async function handleOpen() {
    try {
      const updated = await openDocument(doc!.id)
      setDoc(updated)
      patchDocument(updated)
    } catch (e) {
      toast.error(`Couldn't open: ${e}`)
    }
  }

  async function handleReveal() {
    try {
      await revealInFinder(doc!.path)
    } catch (e) {
      toast.error(`Couldn't reveal: ${e}`)
    }
  }

  async function handleCopyBibtex() {
    try {
      const bib = await exportBibtex(doc!.id)
      await navigator.clipboard.writeText(bib)
      toast.success('BibTeX copied to clipboard')
    } catch (e) {
      toast.error(`Couldn't export BibTeX: ${e}`)
    }
  }

  const fromBibtex = doc.metadataSource === 'bibtex'
  const abstract = doc.abstractText ?? doc.subject

  return (
    <AppShell>
      <div className="max-w-4xl mx-auto px-5 md:px-8 py-6">
        <Link
          to="/"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground mb-5"
        >
          <ArrowLeft className="w-3.5 h-3.5" /> Library
        </Link>

        <div className="flex flex-col sm:flex-row gap-7">
          <div className="w-52 shrink-0">
            <div className="aspect-[3/4] rounded-lg overflow-hidden bg-muted ring-1 ring-black/5 shadow-md">
              {thumb ? (
                <img src={thumb} alt={doc.title} className="w-full h-full object-cover object-top" />
              ) : (
                <div className="w-full h-full flex items-center justify-center">
                  <FileText className="w-8 h-8 text-muted-foreground" />
                </div>
              )}
            </div>
            <div className="flex flex-col gap-2 mt-3">
              <button
                onClick={handleOpen}
                className="flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg bg-primary text-primary-foreground text-sm font-medium hover:opacity-90 transition-opacity"
              >
                <ExternalLink className="w-3.5 h-3.5" /> Open PDF
              </button>
              <button
                onClick={handleReveal}
                className="flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg border border-border text-sm hover:bg-muted transition-colors"
              >
                <FolderOpen className="w-3.5 h-3.5" /> Reveal in Finder
              </button>
              <button
                onClick={handleCopyBibtex}
                className="flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg border border-border text-sm hover:bg-muted transition-colors"
              >
                <Quote className="w-3.5 h-3.5" /> Copy as BibTeX
              </button>
              {!editing && (
                <button
                  onClick={() => setEditing(true)}
                  className="flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg border border-border text-sm hover:bg-muted transition-colors"
                >
                  <Pencil className="w-3.5 h-3.5" /> Edit metadata
                </button>
              )}
            </div>
          </div>

          {editing ? (
            <div className="flex-1 min-w-0">
              <DocumentEditForm doc={doc} onCancel={() => setEditing(false)} onSave={handleSaveEdit} />
            </div>
          ) : (
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 text-xs text-muted-foreground mb-1.5">
              <FolderOpen className="w-3.5 h-3.5" />
              <span>{doc.venue ?? doc.category}</span>
              {doc.year && (
                <>
                  <span>·</span>
                  <span>{doc.year}</span>
                </>
              )}
              {fromBibtex && (
                <span className="flex items-center gap-1 px-1.5 py-0.5 rounded-full bg-primary/10 text-primary text-[0.65rem] font-medium">
                  <BookText className="w-2.5 h-2.5" /> BibTeX
                </span>
              )}
            </div>
            <h1 className="font-display text-xl leading-snug">{doc.title}</h1>
            {doc.authors.length > 0 && (
              <p className="text-sm text-muted-foreground mt-1">{doc.authors.join(', ')}</p>
            )}

            <div className="mt-4">
              <ReadingControls doc={doc} onUpdated={setDoc} />
            </div>

            {abstract && <p className="text-sm mt-4 leading-relaxed">{abstract}</p>}

            {doc.keywords.length > 0 && (
              <div className="flex flex-wrap items-center gap-1.5 mt-4">
                {doc.keywords.map(k => (
                  <span
                    key={k}
                    className="flex items-center gap-1 text-[0.7rem] px-2 py-0.5 rounded-full bg-muted text-muted-foreground"
                  >
                    <Hash className="w-2.5 h-2.5" />
                    {k}
                  </span>
                ))}
              </div>
            )}

            <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 mt-6 text-sm">
              {doc.doi && (
                <>
                  <dt className="text-muted-foreground">DOI</dt>
                  <dd>
                    <a
                      href={`https://doi.org/${doc.doi}`}
                      target="_blank"
                      rel="noreferrer"
                      className="text-primary hover:underline inline-flex items-center gap-1 break-all"
                    >
                      {doc.doi} <ExternalLink className="w-3 h-3 shrink-0" />
                    </a>
                  </dd>
                </>
              )}
              {doc.citationKey && (
                <>
                  <dt className="text-muted-foreground">Citation key</dt>
                  <dd className="font-mono text-xs">{doc.citationKey}</dd>
                </>
              )}

              <dt className="text-muted-foreground">Pages</dt>
              <dd>{doc.pageCount > 0 ? doc.pageCount : 'Unknown'}</dd>

              <dt className="text-muted-foreground">Size</dt>
              <dd>{formatBytes(doc.sizeBytes)}</dd>

              <dt className="text-muted-foreground">Folder</dt>
              <dd className="truncate">
                {doc.category}
                {doc.relativeDir ? ` / ${doc.relativeDir.replace(/\//g, ' / ')}` : ''}
              </dd>

              <dt className="text-muted-foreground">File</dt>
              <dd className="truncate">{doc.fileName}</dd>

              {doc.modifiedAt && (
                <>
                  <dt className="text-muted-foreground">Modified</dt>
                  <dd>{formatDate(doc.modifiedAt)}</dd>
                </>
              )}

              <dt className="text-muted-foreground">Added</dt>
              <dd>{formatDate(doc.addedAt)}</dd>

              <dt className="text-muted-foreground">Location</dt>
              <dd className="text-xs text-muted-foreground break-all">{doc.path}</dd>
            </dl>
          </div>
          )}
        </div>
      </div>
    </AppShell>
  )
}
