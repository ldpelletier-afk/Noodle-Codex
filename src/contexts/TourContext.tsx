import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { OnboardingTour, TOUR_STEPS } from '@/components/OnboardingTour'
import { useLibrary } from '@/contexts/LibraryContext'

// Set once the walkthrough has been finished or skipped, so it only
// auto-starts on a first launch. "Show walkthrough" in Settings replays it.
const TOUR_DONE_KEY = 'codex_tour_done'

// Off for the maintainer's own builds via `VITE_DISABLE_TOUR=1` in a
// git-ignored `.env.local`; on for everyone else, including release builds.
export const TOUR_ENABLED = import.meta.env.VITE_DISABLE_TOUR !== '1'

interface TourContextValue {
  startTour: () => void
}

const TourContext = createContext<TourContextValue | null>(null)

function tourDone(): boolean {
  try {
    return localStorage.getItem(TOUR_DONE_KEY) === '1'
  } catch {
    return true
  }
}

export function TourProvider({ children }: { children: ReactNode }) {
  const navigate = useNavigate()
  const { pathname } = useLocation()
  const { documents, loading } = useLibrary()
  const [step, setStep] = useState<number | null>(null)

  // First launch: start once the library has loaded, so the first step's
  // target is on screen.
  useEffect(() => {
    if (TOUR_ENABLED && !loading && !tourDone()) setStep(0)
  }, [loading])

  const finish = useCallback(() => {
    setStep(null)
    try {
      localStorage.setItem(TOUR_DONE_KEY, '1')
    } catch {
      // Non-fatal: the tour just shows again next launch.
    }
  }, [])

  const goTo = useCallback(
    (next: number) => (next >= TOUR_STEPS.length ? finish() : setStep(Math.max(0, next))),
    [finish]
  )

  // Entering a step: bring up the page its target lives on, if we're not
  // already there. Only on entry — afterwards the reader can wander freely.
  useEffect(() => {
    if (step === null) return
    const s = TOUR_STEPS[step]
    if (s.onRoute && !s.onRoute(pathname)) {
      const to = s.route?.(documents)
      if (to) navigate(to)
    }
  }, [step])

  const startTour = useCallback(() => setStep(0), [])

  return (
    <TourContext.Provider value={{ startTour }}>
      {children}
      {step !== null && (
        <OnboardingTour
          step={step}
          onNext={() => goTo(step + 1)}
          onBack={() => goTo(step - 1)}
          onSkip={finish}
        />
      )}
    </TourContext.Provider>
  )
}

export function useTour() {
  const ctx = useContext(TourContext)
  if (!ctx) throw new Error('useTour must be used within TourProvider')
  return ctx
}
