import { cn } from '@/lib/utils'
import { Textarea } from '@/components/ui/textarea'
import { forwardRef } from 'react'

interface SettingsTextAreaProps extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  label?: string
  error?: string
}

export const SettingsTextArea = forwardRef<HTMLTextAreaElement, SettingsTextAreaProps>(
  ({ label, error, className, ...props }, ref) => {
    return (
      <div className="space-y-1.5">
        {label && (
          <label className="text-sm font-medium text-foreground">{label}</label>
        )}
        <Textarea
          ref={ref}
          className={cn(
            'text-sm min-h-[80px] resize-y',
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

SettingsTextArea.displayName = 'SettingsTextArea'
