/**
 * SettingsBreadcrumb — sticky top header for the settings dialog.
 *
 * Shows: 设置 / <tab label> / <subsection title?>
 * The subsection segment is driven by IntersectionObserver tracking
 * elements with `data-settings-section` attribute inside the
 * scrollable content container.
 */
import * as React from 'react'
import { ChevronRight, X } from 'lucide-react'

interface SettingsBreadcrumbProps {
  tabLabel: string
  /** Scroll container ref — used to observe section markers inside. */
  scrollContainerRef: React.MutableRefObject<HTMLElement | null>
  onClose: () => void
}

export function SettingsBreadcrumb({
  tabLabel,
  scrollContainerRef,
  onClose,
}: SettingsBreadcrumbProps): React.ReactElement {
  const [activeSection, setActiveSection] = React.useState<string | null>(null)

  React.useEffect(() => {
    setActiveSection(null)
    const root = scrollContainerRef.current
    if (!root) return

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((e) => e.isIntersecting)
          .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)
        if (visible.length > 0) {
          const el = visible[0]!.target as HTMLElement
          setActiveSection(el.dataset.settingsSection ?? null)
        }
      },
      { root, rootMargin: '0px 0px -70% 0px', threshold: 0 },
    )

    // Defer to next frame so the new tab's DOM is mounted.
    const id = requestAnimationFrame(() => {
      const nodes = root.querySelectorAll<HTMLElement>('[data-settings-section]')
      nodes.forEach((n) => observer.observe(n))
    })

    return () => {
      cancelAnimationFrame(id)
      observer.disconnect()
    }
  }, [scrollContainerRef, tabLabel])

  return (
    <div className="h-12 flex items-center justify-between px-5 border-b border-border/50 flex-shrink-0 bg-background/95 backdrop-blur-sm sticky top-0 z-10">
      <div className="flex items-center gap-1.5 text-sm">
        <span className="text-muted-foreground">设置</span>
        <ChevronRight size={12} className="text-muted-foreground/50" />
        <span className="text-foreground font-medium">{tabLabel}</span>
        {activeSection && (
          <>
            <ChevronRight size={12} className="text-muted-foreground/50" />
            <span className="text-foreground/80">{activeSection}</span>
          </>
        )}
      </div>
      <button
        type="button"
        aria-label="关闭"
        onClick={onClose}
        className="rounded-md p-1.5 text-muted-foreground/60 hover:text-foreground hover:bg-muted transition-colors"
      >
        <X size={16} />
      </button>
    </div>
  )
}
