import type { Document } from './types'

export interface SubfolderEntry {
  /** Display name of the immediate child folder. */
  name: string
  /** Full relative path of the child folder (e.g. "Sources/Primary"). */
  path: string
  /** Number of documents anywhere beneath this folder. */
  count: number
}

export interface Crumb {
  name: string
  /** Relative dir this crumb navigates to ("" for the collection root). */
  path: string
}

function childPrefix(currentDir: string): string {
  return currentDir === '' ? '' : `${currentDir}/`
}

/** Documents that live directly in `currentDir` (not in a nested subfolder). */
export function documentsInDir(docs: Document[], currentDir: string): Document[] {
  return docs.filter(d => d.relativeDir === currentDir)
}

/** Documents in `currentDir` and every folder nested beneath it — the "this
 * folder and its contents" scope used for exporting a .bib of the current view. */
export function documentsUnderDir(docs: Document[], currentDir: string): Document[] {
  if (currentDir === '') return docs
  const prefix = childPrefix(currentDir)
  return docs.filter(d => d.relativeDir === currentDir || d.relativeDir.startsWith(prefix))
}

/** Immediate child folders of `currentDir`, each with a recursive document count. */
export function immediateSubfolders(docs: Document[], currentDir: string): SubfolderEntry[] {
  const prefix = childPrefix(currentDir)
  const counts = new Map<string, number>()

  for (const d of docs) {
    if (d.relativeDir === currentDir) continue
    if (currentDir !== '' && !d.relativeDir.startsWith(prefix)) continue
    const rest = d.relativeDir.slice(prefix.length)
    if (!rest) continue
    const name = rest.split('/')[0]
    if (!name) continue
    counts.set(name, (counts.get(name) ?? 0) + 1)
  }

  return [...counts.entries()]
    .map(([name, count]) => ({ name, path: prefix + name, count }))
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }))
}

/** Breadcrumb trail from the collection root down to `currentDir`. */
export function breadcrumbs(rootName: string, currentDir: string): Crumb[] {
  const crumbs: Crumb[] = [{ name: rootName, path: '' }]
  if (currentDir === '') return crumbs
  const segments = currentDir.split('/')
  let acc = ''
  for (const seg of segments) {
    acc = acc === '' ? seg : `${acc}/${seg}`
    crumbs.push({ name: seg, path: acc })
  }
  return crumbs
}
