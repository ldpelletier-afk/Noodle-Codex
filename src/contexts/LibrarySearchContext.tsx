import { createContext, useContext, useState, type ReactNode } from 'react'

interface LibrarySearchValue {
  query: string
  setQuery: (q: string) => void
}

const LibrarySearchContext = createContext<LibrarySearchValue>({ query: '', setQuery: () => {} })

export function LibrarySearchProvider({ children }: { children: ReactNode }) {
  const [query, setQuery] = useState('')
  return (
    <LibrarySearchContext.Provider value={{ query, setQuery }}>
      {children}
    </LibrarySearchContext.Provider>
  )
}

export function useLibrarySearch() {
  return useContext(LibrarySearchContext)
}
