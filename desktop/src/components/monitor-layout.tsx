import { cn } from "@/lib/utils"
import type { MonitorsResult } from "@/lib/engine"

/**
 * Scaled map of the physical monitor arrangement.
 *
 * Positions come straight from the engine's virtual-desktop geometry, so monitors
 * with negative offsets (a taller display sitting above the primary) land where they
 * actually are rather than being normalised into a row.
 */
export function MonitorLayout({
  monitors,
  className,
}: {
  monitors: MonitorsResult
  className?: string
}) {
  const { monitors: list, virtual_width, virtual_height } = monitors
  if (list.length === 0 || virtual_width === 0) return null

  const minX = Math.min(...list.map((m) => m.x))
  const minY = Math.min(...list.map((m) => m.y))
  const aspect = (virtual_height / virtual_width) * 100

  return (
    <div className={cn("relative w-full rounded-md border bg-muted/20", className)}>
      <div style={{ paddingBottom: `${aspect}%` }} />
      {list.map((m) => (
        <div
          key={m.index}
          className="absolute flex flex-col items-center justify-center rounded-sm border border-border bg-primary/15 text-center"
          style={{
            left: `${((m.x - minX) / virtual_width) * 100}%`,
            top: `${((m.y - minY) / virtual_height) * 100}%`,
            width: `${(m.width / virtual_width) * 100}%`,
            height: `${(m.height / virtual_height) * 100}%`,
          }}
        >
          <span className="text-xs font-medium">#{m.index + 1}</span>
          <span className="text-[10px] text-muted-foreground">
            {m.width}×{m.height}
          </span>
        </div>
      ))}
    </div>
  )
}
