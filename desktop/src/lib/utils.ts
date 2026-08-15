import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

/** Last segment of a path. Handles both separators — the engine reports Windows ones. */
export function basename(path: string): string {
  const at = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"))
  return at >= 0 ? path.slice(at + 1) : path
}
