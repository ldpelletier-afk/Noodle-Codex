/** True on macOS. Skim, Finder and Preview only exist there; elsewhere the UI
 * uses the platform's own names and hides Mac-only settings. */
export const IS_MAC = /Mac/i.test(navigator.userAgent)

/** Last component of a file path, whichever separator the OS uses. */
export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path
}
