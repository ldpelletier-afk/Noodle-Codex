import { createContext, useCallback, useContext, useState, type ReactNode } from 'react'
import { CheckCircle2, Info, AlertTriangle, XCircle, X } from 'lucide-react'
import { cn } from '@/lib/utils'

type ToastKind = 'success' | 'info' | 'warning' | 'error'

interface Toast {
  id: number
  kind: ToastKind
  message: string
}

interface ToastApi {
  success: (message: string) => void
  info: (message: string) => void
  warning: (message: string) => void
  error: (message: string) => void
}

const ToastContext = createContext<{ toast: ToastApi }>({
  toast: { success: () => {}, info: () => {}, warning: () => {}, error: () => {} },
})

const ICONS: Record<ToastKind, typeof Info> = {
  success: CheckCircle2,
  info: Info,
  warning: AlertTriangle,
  error: XCircle,
}

let nextId = 1

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([])

  const push = useCallback((kind: ToastKind, message: string) => {
    const id = nextId++
    setToasts(t => [...t, { id, kind, message }])
    setTimeout(() => setToasts(t => t.filter(x => x.id !== id)), 4000)
  }, [])

  const toast: ToastApi = {
    success: m => push('success', m),
    info: m => push('info', m),
    warning: m => push('warning', m),
    error: m => push('error', m),
  }

  return (
    <ToastContext.Provider value={{ toast }}>
      {children}
      <div className="fixed bottom-5 left-1/2 -translate-x-1/2 z-50 flex flex-col gap-2 items-center pointer-events-none">
        {toasts.map(t => {
          const Icon = ICONS[t.kind]
          return (
            <div
              key={t.id}
              className={cn(
                'pointer-events-auto flex items-center gap-2 px-3.5 py-2 rounded-lg border border-border bg-card shadow-lg text-sm animate-card-appear',
                t.kind === 'success' && 'text-success',
                t.kind === 'info' && 'text-info',
                t.kind === 'warning' && 'text-warning',
                t.kind === 'error' && 'text-destructive'
              )}
            >
              <Icon className="w-4 h-4 shrink-0" />
              <span className="text-foreground">{t.message}</span>
              <button
                onClick={() => setToasts(ts => ts.filter(x => x.id !== t.id))}
                className="text-muted-foreground hover:text-foreground ml-1"
              >
                <X className="w-3 h-3" />
              </button>
            </div>
          )
        })}
      </div>
    </ToastContext.Provider>
  )
}

export function useToast() {
  return useContext(ToastContext)
}
