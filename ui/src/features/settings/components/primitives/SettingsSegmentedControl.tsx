import { cn } from '@/lib/utils'

interface SegmentOption {
  value: string
  label: string
}

interface SettingsSegmentedControlProps {
  value: string
  onValueChange: (value: string) => void
  options: SegmentOption[]
  className?: string
}

export function SettingsSegmentedControl({
  value,
  onValueChange,
  options,
  className,
}: SettingsSegmentedControlProps) {
  return (
    <div
      className={cn(
        'inline-flex items-center rounded-lg bg-muted p-1 gap-0.5',
        className
      )}
    >
      {options.map((opt) => (
        <button
          key={opt.value}
          type="button"
          className={cn(
            'px-3 py-1.5 text-xs font-medium rounded-md transition-colors',
            value === opt.value
              ? 'bg-background text-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground'
          )}
          onClick={() => onValueChange(opt.value)}
        >
          {opt.label}
        </button>
      ))}
    </div>
  )
}
