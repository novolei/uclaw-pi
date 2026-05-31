import { cn } from '@/lib/utils'
import { Input } from '@/components/ui/input'
import { forwardRef } from 'react'

interface SettingsInputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  label?: string
  error?: string
}

export const SettingsInput = forwardRef<HTMLInputElement, SettingsInputProps>(
  ({ label, error, className, ...props }, ref) => {
    return (
      <div className="space-y-1.5">
        {label && (
          <label className="text-sm font-medium text-foreground">{label}</label>
        )}
        <Input
          ref={ref}
          className={cn(
            'h-9 text-sm',
            error && 'border-destructive',
            className
          )}
          {...props}
        />
        {error && (
          <p className="text-xs text-destructive">{error}</p>
        )}
      </div>
    )
  }
)

SettingsInput.displayName = 'SettingsInput'
