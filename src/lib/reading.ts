import type { ReadingEvent } from './types'

export interface DayBucket {
  /** YYYY-MM-DD (local) */
  date: string
  count: number
}

function localDayKey(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso.slice(0, 10)
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}

function addDays(key: string, delta: number): string {
  const d = new Date(key + 'T00:00:00')
  d.setDate(d.getDate() + delta)
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}

function todayKey(): string {
  return localDayKey(new Date().toISOString())
}

/** Counts "open" events per calendar day over the trailing `days` window,
 * filling gaps with zeros so a heatmap and streaks have a continuous range. */
export function dailyActivity(events: ReadingEvent[], days = 119): DayBucket[] {
  const counts = new Map<string, number>()
  for (const e of events) {
    if (e.kind !== 'open') continue
    const key = localDayKey(e.at)
    counts.set(key, (counts.get(key) ?? 0) + 1)
  }

  const end = todayKey()
  const start = addDays(end, -(days - 1))
  const buckets: DayBucket[] = []
  let cursor = start
  while (cursor <= end) {
    buckets.push({ date: cursor, count: counts.get(cursor) ?? 0 })
    cursor = addDays(cursor, 1)
  }
  return buckets
}

/** Reading streak counting back from the most recent active day (today need
 * not be active for an ongoing streak to count). */
export function currentStreak(buckets: DayBucket[]): number {
  const lastActiveFromEnd = [...buckets].reverse().findIndex(b => b.count > 0)
  if (lastActiveFromEnd === -1) return 0
  let streak = 0
  for (let i = buckets.length - 1 - lastActiveFromEnd; i >= 0; i--) {
    if (buckets[i].count > 0) streak++
    else break
  }
  return streak
}

export function longestStreak(buckets: DayBucket[]): number {
  let longest = 0
  let run = 0
  for (const b of buckets) {
    if (b.count > 0) {
      run++
      longest = Math.max(longest, run)
    } else {
      run = 0
    }
  }
  return longest
}

/** True if the ISO timestamp falls within the trailing `days` window. */
export function withinDays(iso: string | null, days: number): boolean {
  if (!iso) return false
  const t = new Date(iso).getTime()
  if (Number.isNaN(t)) return false
  return t >= Date.now() - days * 24 * 60 * 60 * 1000
}
