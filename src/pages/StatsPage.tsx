import { useMemo } from 'react'
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from 'recharts'
import { FileText, Layers, FolderOpen, HardDrive, BookOpen, CheckCircle2, Flame, Trophy } from 'lucide-react'
import { Link } from 'react-router-dom'
import { AppShell } from '@/components/AppShell'
import { StatCard } from '@/components/stats/StatCard'
import { BreakdownList } from '@/components/stats/BreakdownList'
import { CalendarHeatmap } from '@/components/stats/CalendarHeatmap'
import { useLibrary } from '@/contexts/LibraryContext'
import { formatBytes, formatDate } from '@/lib/utils'
import { dailyActivity, currentStreak, longestStreak, withinDays } from '@/lib/reading'

export function StatsPage() {
  const { documents, folders, readingEvents, loading } = useLibrary()

  const totalPages = useMemo(() => documents.reduce((s, d) => s + d.pageCount, 0), [documents])
  const totalBytes = useMemo(() => documents.reduce((s, d) => s + d.sizeBytes, 0), [documents])

  // --- Reading stats ---
  const activity = useMemo(() => dailyActivity(readingEvents, 119), [readingEvents])
  const streak = useMemo(() => currentStreak(activity), [activity])
  const longest = useMemo(() => longestStreak(activity), [activity])
  const readingNow = useMemo(() => documents.filter(d => d.status === 'in_progress'), [documents])
  const finishedCount = useMemo(() => documents.filter(d => d.status === 'completed').length, [documents])
  const finishedThisMonth = useMemo(
    () => documents.filter(d => d.status === 'completed' && withinDays(d.finishedAt, 30)).length,
    [documents]
  )
  const continueReading = useMemo(
    () =>
      [...readingNow]
        .sort((a, b) => (a.lastReadAt ?? '') < (b.lastReadAt ?? '') ? 1 : -1)
        .slice(0, 6),
    [readingNow]
  )
  const recentlyFinished = useMemo(
    () =>
      documents
        .filter(d => d.status === 'completed' && d.finishedAt)
        .sort((a, b) => ((a.finishedAt ?? '') < (b.finishedAt ?? '') ? 1 : -1))
        .slice(0, 6),
    [documents]
  )
  const hasReadingActivity = readingEvents.length > 0 || readingNow.length > 0 || finishedCount > 0

  const byCollection = useMemo(
    () =>
      folders
        .map(f => ({ label: f.name, value: f.documentCount }))
        .sort((a, b) => b.value - a.value),
    [folders]
  )

  const byYear = useMemo(() => {
    const counts = new Map<number, number>()
    for (const d of documents) {
      if (d.year) counts.set(d.year, (counts.get(d.year) ?? 0) + 1)
    }
    return [...counts.entries()]
      .sort((a, b) => a[0] - b[0])
      .map(([year, count]) => ({ year: String(year), count }))
  }, [documents])

  const largest = useMemo(
    () => [...documents].sort((a, b) => b.sizeBytes - a.sizeBytes).slice(0, 6),
    [documents]
  )

  const recent = useMemo(
    () =>
      [...documents]
        .sort((a, b) => (a.addedAt < b.addedAt ? 1 : -1))
        .slice(0, 6),
    [documents]
  )

  return (
    <AppShell>
      <div className="max-w-6xl mx-auto px-5 md:px-8 py-6">
        <div className="mb-5">
          <h1 className="font-display text-2xl">Library insights</h1>
          <p className="text-sm text-muted-foreground mt-0.5">A read on your whole collection.</p>
        </div>

        {loading ? (
          <div className="grid sm:grid-cols-4 gap-4">
            {Array.from({ length: 4 }).map((_, i) => (
              <div key={i} className="h-24 rounded-xl bg-muted animate-pulse" />
            ))}
          </div>
        ) : documents.length === 0 ? (
          <p className="text-sm text-muted-foreground py-16 text-center">
            Add a folder of PDFs to see insights.
          </p>
        ) : (
          <>
            {/* Reading */}
            <h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2">
              Reading
            </h2>
            {hasReadingActivity ? (
              <>
                <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
                  <StatCard label="Currently reading" value={readingNow.length.toLocaleString()} icon={BookOpen} />
                  <StatCard
                    label="Finished"
                    value={finishedCount.toLocaleString()}
                    sublabel={finishedThisMonth > 0 ? `${finishedThisMonth} in the last 30 days` : undefined}
                    icon={CheckCircle2}
                  />
                  <StatCard label="Current streak" value={`${streak} day${streak === 1 ? '' : 's'}`} icon={Flame} />
                  <StatCard label="Longest streak" value={`${longest} day${longest === 1 ? '' : 's'}`} icon={Trophy} />
                </div>

                <div className="rounded-xl border border-border bg-card p-4 mt-4">
                  <p className="text-xs font-medium text-muted-foreground mb-3">Reading activity</p>
                  <div className="h-40">
                    <CalendarHeatmap buckets={activity} />
                  </div>
                </div>

                <div className="grid lg:grid-cols-2 gap-4 mt-4">
                  <DocList
                    title="Continue reading"
                    docs={continueReading}
                    emptyLabel="Nothing in progress."
                    trailing={d => (d.pageCount > 0 ? `${Math.round((d.currentPage / d.pageCount) * 100)}%` : 'reading')}
                  />
                  <DocList
                    title="Recently finished"
                    docs={recentlyFinished}
                    emptyLabel="No finished documents yet."
                    trailing={d => (d.finishedAt ? formatDate(d.finishedAt) : '')}
                  />
                </div>
              </>
            ) : (
              <p className="text-sm text-muted-foreground rounded-xl border border-border bg-card p-4">
                Open a PDF or set its progress to start tracking your reading. Your activity, streaks,
                and progress will appear here.
              </p>
            )}

            {/* Library */}
            <h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2 mt-8">
              Library
            </h2>
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
              <StatCard label="Documents" value={documents.length.toLocaleString()} icon={FileText} />
              <StatCard label="Total pages" value={totalPages.toLocaleString()} icon={Layers} />
              <StatCard label="Collections" value={folders.length.toLocaleString()} icon={FolderOpen} />
              <StatCard label="On disk" value={formatBytes(totalBytes)} icon={HardDrive} />
            </div>

            <div className="grid lg:grid-cols-2 gap-4 mt-4">
              <div className="rounded-xl border border-border bg-card p-4">
                <p className="text-xs font-medium text-muted-foreground mb-3">Documents by collection</p>
                <BreakdownList rows={byCollection} formatValue={v => `${v}`} />
              </div>

              <div className="rounded-xl border border-border bg-card p-4">
                <p className="text-xs font-medium text-muted-foreground mb-3">Documents by year</p>
                {byYear.length === 0 ? (
                  <p className="text-xs text-muted-foreground">No year metadata found yet.</p>
                ) : (
                  <div className="h-48">
                    <ResponsiveContainer width="100%" height="100%">
                      <BarChart data={byYear} margin={{ top: 4, right: 8, left: -22, bottom: 0 }}>
                        <CartesianGrid vertical={false} stroke="var(--border)" />
                        <XAxis
                          dataKey="year"
                          tick={{ fontSize: 10, fill: 'var(--muted-foreground)' }}
                          interval="preserveStartEnd"
                        />
                        <YAxis tick={{ fontSize: 10, fill: 'var(--muted-foreground)' }} width={28} allowDecimals={false} />
                        <Tooltip
                          formatter={(v) => [`${v} docs`, 'Count']}
                          contentStyle={{ background: 'var(--card)', border: '1px solid var(--border)', borderRadius: 8, fontSize: 12 }}
                        />
                        <Bar dataKey="count" fill="var(--chart-accent)" radius={[3, 3, 0, 0]} />
                      </BarChart>
                    </ResponsiveContainer>
                  </div>
                )}
              </div>
            </div>

            <div className="grid lg:grid-cols-2 gap-4 mt-4">
              <DocList title="Recently added" docs={recent} trailing={d => d.category} />
              <DocList title="Largest files" docs={largest} trailing={d => formatBytes(d.sizeBytes)} />
            </div>
          </>
        )}
      </div>
    </AppShell>
  )
}

function DocList({
  title,
  docs,
  trailing,
  emptyLabel,
}: {
  title: string
  docs: import('@/lib/types').Document[]
  trailing: (d: import('@/lib/types').Document) => string
  emptyLabel?: string
}) {
  return (
    <div className="rounded-xl border border-border bg-card p-4">
      <p className="text-xs font-medium text-muted-foreground mb-2">{title}</p>
      {docs.length === 0 ? (
        <p className="text-xs text-muted-foreground py-2">{emptyLabel ?? 'Nothing here yet.'}</p>
      ) : (
        <ul className="flex flex-col">
          {docs.map(d => (
            <li key={d.id}>
              <Link
                to={`/documents/${d.id}`}
                className="flex items-center justify-between gap-3 py-1.5 text-sm hover:text-primary transition-colors"
              >
                <span className="truncate">{d.title}</span>
                <span className="text-xs text-muted-foreground shrink-0">{trailing(d)}</span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
