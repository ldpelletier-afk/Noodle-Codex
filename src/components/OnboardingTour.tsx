import { useEffect, useRef, useState } from 'react'
import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp, X } from 'lucide-react'
import type { Document } from '@/lib/types'
import { cn } from '@/lib/utils'

type Side = 'top' | 'bottom' | 'left' | 'right'

interface TourStep {
  /** `data-tour` values to point at, in order of preference. */
  targets: string[]
  title: string
  body: string
  /** Shown instead of `body` when no target is on screen (e.g. empty library). */
  fallback?: string
  /** Preferred side of the target for the card; flips if it doesn't fit. */
  side: 'bottom' | 'right'
  /** Whether the target lives on the current route; absent = any route. */
  onRoute?: (pathname: string) => boolean
  /** Where to go when entering the step from another route. */
  route?: (documents: Document[]) => string | null
}

export const TOUR_STEPS: TourStep[] = [
  {
    targets: ['add-folder'],
    title: 'Press this button to add your PDFs',
    body: 'Pick any folder on your Mac. Codex indexes every PDF inside it, subfolders included, and keeps watching it for changes.',
    side: 'bottom',
    onRoute: p => p === '/',
    route: () => '/',
  },
  {
    targets: ['nav-library'],
    title: 'Click here to view your library',
    body: 'Everything you’ve added lives here, with a cover for each PDF.',
    side: 'right',
  },
  {
    targets: ['document-card', 'folder-tile'],
    title: 'Click on a book',
    body: 'Each book opens its own page with its details, citation, and reading progress.',
    fallback: 'Once you’ve added a folder, your books show up here. Click any one to see its details.',
    side: 'right',
    onRoute: p => p === '/',
    route: () => '/',
  },
  {
    targets: ['open-pdf'],
    title: 'Open a PDF',
    body: 'Opens the file in your PDF reader. You can choose which app in Settings.',
    fallback: 'Each book’s page has an “Open PDF” button that opens it in your PDF reader.',
    side: 'right',
    onRoute: p => p.startsWith('/documents/'),
    route: docs => (docs[0] ? `/documents/${docs[0].id}` : null),
  },
  {
    targets: ['nav-reading'],
    title: 'Click on Currently reading to see your reading history',
    body: 'Books you’ve opened land here, so you can pick up where you left off.',
    side: 'right',
  },
]

const CARD_WIDTH = 320
const GAP = 14 // highlight → arrow → card spacing
const ARROW = 30
const PAD = 6 // highlight padding around the target
const MARGIN = 12 // min distance from the window edge

interface Rect {
  top: number
  left: number
  width: number
  height: number
}

function findTarget(targets: string[]): HTMLElement | null {
  for (const t of targets) {
    for (const el of document.querySelectorAll<HTMLElement>(`[data-tour="${t}"]`)) {
      const r = el.getBoundingClientRect()
      if (r.width > 0 && r.height > 0) return el
    }
  }
  return null
}

function sameRect(a: Rect | null, b: Rect | null) {
  if (!a || !b) return a === b
  return a.top === b.top && a.left === b.left && a.width === b.width && a.height === b.height
}

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(v, Math.max(lo, hi)))

/** Lays out the card and arrow around the (padded) target rect. */
function layout(r: Rect, preferred: 'bottom' | 'right', card: { w: number; h: number }) {
  const vw = window.innerWidth
  const vh = window.innerHeight
  const reach = GAP + ARROW + GAP

  let side: Side = preferred
  if (side === 'bottom' && r.top + r.height + reach + card.h > vh - MARGIN) side = 'top'
  if (side === 'right' && r.left + r.width + reach + card.w > vw - MARGIN) side = 'left'

  const cx = r.left + r.width / 2
  const cy = r.top + r.height / 2
  let cardPos: { top: number; left: number }
  let arrowPos: { top: number; left: number }

  if (side === 'bottom' || side === 'top') {
    const left = clamp(cx - card.w / 2, MARGIN, vw - card.w - MARGIN)
    if (side === 'bottom') {
      arrowPos = { top: r.top + r.height + GAP, left: cx - ARROW / 2 }
      cardPos = { top: r.top + r.height + reach, left }
    } else {
      arrowPos = { top: r.top - GAP - ARROW, left: cx - ARROW / 2 }
      cardPos = { top: r.top - reach - card.h, left }
    }
  } else {
    const top = clamp(cy - card.h / 2, MARGIN, vh - card.h - MARGIN)
    if (side === 'right') {
      arrowPos = { top: cy - ARROW / 2, left: r.left + r.width + GAP }
      cardPos = { top, left: r.left + r.width + reach }
    } else {
      arrowPos = { top: cy - ARROW / 2, left: r.left - GAP - ARROW }
      cardPos = { top, left: r.left - reach - card.w }
    }
  }
  return { side, cardPos, arrowPos }
}

// The arrow sits on `side` of the target, so it points back the other way.
const ARROW_ICON = { bottom: ArrowUp, top: ArrowDown, right: ArrowLeft, left: ArrowRight }
const NUDGE = {
  bottom: { x: '0px', y: '-6px' },
  top: { x: '0px', y: '6px' },
  right: { x: '-6px', y: '0px' },
  left: { x: '6px', y: '0px' },
}

export function OnboardingTour({
  step,
  onNext,
  onBack,
  onSkip,
}: {
  step: number
  onNext: () => void
  onBack: () => void
  onSkip: () => void
}) {
  const s = TOUR_STEPS[step]
  const last = step === TOUR_STEPS.length - 1
  const [rect, setRect] = useState<Rect | null>(null)
  const [cardH, setCardH] = useState(180)
  const cardRef = useRef<HTMLDivElement>(null)
  const scrolledFor = useRef<number | null>(null)

  // Track the target every frame: it may mount late (route change, library
  // loading), move (scrolling, resizing), or disappear.
  useEffect(() => {
    let frame = 0
    let prev: Rect | null = null
    const tick = () => {
      const el = findTarget(s.targets)
      if (el && scrolledFor.current !== step) {
        scrolledFor.current = step
        el.scrollIntoView({ block: 'nearest', inline: 'nearest' })
      }
      const r = el?.getBoundingClientRect()
      const next = r
        ? {
            top: Math.round(r.top) - PAD,
            left: Math.round(r.left) - PAD,
            width: Math.round(r.width) + PAD * 2,
            height: Math.round(r.height) + PAD * 2,
          }
        : null
      if (!sameRect(prev, next)) {
        prev = next
        setRect(next)
      }
      const h = cardRef.current?.offsetHeight
      if (h) setCardH(cur => (cur === h ? cur : h))
      frame = requestAnimationFrame(tick)
    }
    tick()
    return () => cancelAnimationFrame(frame)
  }, [s, step])

  // Doing what the step asks (clicking the highlighted thing) advances the
  // tour. Capture phase, so we see the click even if it unmounts the target.
  useEffect(() => {
    const handleClick = (e: MouseEvent) => {
      const el = findTarget(s.targets)
      if (el && el.contains(e.target as Node)) {
        // Let the click's own navigation land before the next step reads the route.
        setTimeout(onNext, 50)
      }
    }
    document.addEventListener('click', handleClick, true)
    return () => document.removeEventListener('click', handleClick, true)
  }, [s, onNext])

  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onSkip()
      else if (e.key === 'ArrowRight') onNext()
      else if (e.key === 'ArrowLeft' && step > 0) onBack()
    }
    window.addEventListener('keydown', handleKey)
    return () => window.removeEventListener('keydown', handleKey)
  }, [step, onNext, onBack, onSkip])

  const placed = rect ? layout(rect, s.side, { w: CARD_WIDTH, h: cardH }) : null
  const Arrow = placed ? ARROW_ICON[placed.side] : null

  const card = (
    <div
      ref={cardRef}
      role="dialog"
      aria-label={`Walkthrough step ${step + 1} of ${TOUR_STEPS.length}`}
      className="pointer-events-auto bg-card border border-border rounded-xl shadow-xl p-4 transition-[top,left] duration-200"
      style={
        placed
          ? { position: 'fixed', width: CARD_WIDTH, ...placed.cardPos }
          : { position: 'relative', width: CARD_WIDTH }
      }
    >
      <div className="flex items-start justify-between gap-3 mb-1.5">
        <p className="text-xs font-medium text-primary tabular-nums">
          Step {step + 1} of {TOUR_STEPS.length}
        </p>
        <button
          onClick={onSkip}
          title="Close walkthrough (Esc)"
          className="-mt-1 -mr-1 w-6 h-6 flex items-center justify-center rounded text-muted-foreground hover:text-foreground hover:bg-muted"
        >
          <X className="w-3.5 h-3.5" />
        </button>
      </div>
      <h2 className="font-display text-base leading-snug">{s.title}</h2>
      <p className="text-sm text-muted-foreground mt-1">{rect ? s.body : (s.fallback ?? s.body)}</p>
      <div className="flex items-center justify-between mt-4">
        <div className="flex gap-1">
          {TOUR_STEPS.map((_, i) => (
            <span
              key={i}
              className={cn('h-1.5 rounded-full transition-all', i === step ? 'w-4 bg-primary' : 'w-1.5 bg-border')}
            />
          ))}
        </div>
        <div className="flex gap-2">
          {step === 0 ? (
            <button
              onClick={onSkip}
              className="px-3 py-1.5 rounded-lg text-sm text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
            >
              Skip
            </button>
          ) : (
            <button
              onClick={onBack}
              className="px-3 py-1.5 rounded-lg border border-border text-sm hover:bg-muted transition-colors"
            >
              Back
            </button>
          )}
          <button
            onClick={onNext}
            className="px-3 py-1.5 rounded-lg bg-primary text-primary-foreground text-sm font-medium hover:opacity-90 transition-opacity"
          >
            {last ? 'Done' : 'Next'}
          </button>
        </div>
      </div>
    </div>
  )

  // No target on screen: a centered card over a plain dim.
  if (!placed || !rect || !Arrow) {
    return (
      <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/45 pointer-events-none">
        {card}
      </div>
    )
  }

  // The dim is the highlight's giant box-shadow, so the target stays bright
  // and — with pointer-events off — clickable.
  return (
    <div className="fixed inset-0 z-[60] pointer-events-none">
      <div
        className="tour-ring fixed rounded-xl border-2 border-primary transition-all duration-200"
        style={{ ...rect, boxShadow: '0 0 0 9999px rgb(0 0 0 / 0.45)' }}
      />
      <div className="fixed transition-[top,left] duration-200" style={{ ...placed.arrowPos, width: ARROW, height: ARROW }}>
        <div
          className="tour-arrow w-full h-full rounded-full bg-primary text-primary-foreground shadow-lg flex items-center justify-center"
          style={
            {
              '--nudge-x': NUDGE[placed.side].x,
              '--nudge-y': NUDGE[placed.side].y,
            } as React.CSSProperties
          }
        >
          <Arrow className="w-4 h-4" strokeWidth={3} />
        </div>
      </div>
      {card}
    </div>
  )
}
