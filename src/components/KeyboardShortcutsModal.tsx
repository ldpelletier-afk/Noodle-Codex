import { X } from 'lucide-react'

const SHORTCUTS: [string, string][] = [
  ['/', 'Focus search'],
  ['?', 'Toggle this help'],
  ['Esc', 'Close dialogs'],
]

export function KeyboardShortcutsModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  if (!open) return null
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/40" onClick={onClose} />
      <div className="relative bg-card border border-border rounded-xl shadow-xl w-80 p-5">
        <div className="flex items-center justify-between mb-3">
          <h2 className="font-display text-base">Keyboard shortcuts</h2>
          <button onClick={onClose} className="text-muted-foreground hover:text-foreground">
            <X className="w-4 h-4" />
          </button>
        </div>
        <dl className="flex flex-col gap-2">
          {SHORTCUTS.map(([key, desc]) => (
            <div key={key} className="flex items-center justify-between text-sm">
              <dt className="text-muted-foreground">{desc}</dt>
              <dd className="font-mono px-1.5 py-0.5 rounded border border-border bg-muted text-xs">{key}</dd>
            </div>
          ))}
        </dl>
      </div>
    </div>
  )
}
