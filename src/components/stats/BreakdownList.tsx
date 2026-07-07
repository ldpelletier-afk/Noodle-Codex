export interface BreakdownRow {
  label: string
  value: number
}

/** Shared horizontal-bar list — used for both "by status" and "by device"
 * breakdowns so the two widgets read as one family. */
export function BreakdownList({ rows, formatValue }: { rows: BreakdownRow[]; formatValue: (v: number) => string }) {
  const max = Math.max(...rows.map(r => r.value), 1)

  if (rows.length === 0) {
    return <p className="text-xs text-muted-foreground">No data yet.</p>
  }

  return (
    <div className="flex flex-col gap-2.5 justify-center h-full">
      {rows.map(r => (
        <div key={r.label}>
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="font-medium truncate">{r.label}</span>
            <span className="text-muted-foreground shrink-0 ml-2">{formatValue(r.value)}</span>
          </div>
          <div className="h-1.5 rounded-full bg-muted overflow-hidden">
            <div
              className="h-full rounded-full bg-primary"
              style={{ width: `${(r.value / max) * 100}%` }}
            />
          </div>
        </div>
      ))}
    </div>
  )
}
