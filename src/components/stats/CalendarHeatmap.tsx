import type { DayBucket } from '@/lib/reading'

const DOW_LABELS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']
const LEVEL_OPACITY = [0, 0.3, 0.5, 0.75, 1]

function levelFor(count: number): number {
  if (count <= 0) return 0
  if (count === 1) return 1
  if (count === 2) return 2
  if (count <= 4) return 3
  return 4
}

/** GitHub-contribution-style calendar of reading activity: one column per week,
 * one row per weekday, cell shade = number of opens that day. */
export function CalendarHeatmap({ buckets }: { buckets: DayBucket[] }) {
  if (buckets.length === 0) return null

  const firstDow = new Date(buckets[0].date + 'T00:00:00').getDay()
  const padded: (DayBucket | null)[] = [...Array(firstDow).fill(null), ...buckets]

  const weeks: (DayBucket | null)[][] = []
  for (let i = 0; i < padded.length; i += 7) {
    weeks.push(padded.slice(i, i + 7))
  }

  return (
    <div className="h-full flex flex-col">
      <div className="flex-1 flex items-stretch gap-[3px] overflow-x-auto">
        <div className="flex flex-col justify-between py-[3px] pr-1 shrink-0">
          {DOW_LABELS.map((d, i) => (
            <span key={d} className="text-[0.6rem] text-muted-foreground leading-none h-3 flex items-center">
              {i % 2 === 1 ? d.slice(0, 1) : ''}
            </span>
          ))}
        </div>
        <div className="flex gap-[3px]">
          {weeks.map((week, wi) => (
            <div key={wi} className="flex flex-col gap-[3px]">
              {week.map((bucket, di) => {
                if (!bucket) return <div key={di} className="w-3 h-3" />
                const level = levelFor(bucket.count)
                return (
                  <div
                    key={di}
                    title={`${bucket.date} — ${bucket.count} open${bucket.count === 1 ? '' : 's'}`}
                    className="w-3 h-3 rounded-[2px]"
                    style={{
                      background: level === 0 ? 'var(--muted)' : 'var(--chart-accent)',
                      opacity: level === 0 ? 1 : LEVEL_OPACITY[level],
                    }}
                  />
                )
              })}
            </div>
          ))}
        </div>
      </div>
      <div className="flex items-center gap-1.5 mt-2 text-[0.65rem] text-muted-foreground">
        <span>Less</span>
        {LEVEL_OPACITY.map((op, i) => (
          <div
            key={i}
            className="w-2.5 h-2.5 rounded-[2px]"
            style={{ background: i === 0 ? 'var(--muted)' : 'var(--chart-accent)', opacity: i === 0 ? 1 : op }}
          />
        ))}
        <span>More</span>
      </div>
    </div>
  )
}
