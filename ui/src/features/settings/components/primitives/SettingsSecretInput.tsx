import { cn } from '@/lib/utils'
import { forwardRef, useState } from 'react'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'

interface SettingsSecretInputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'type'> {
  label?: string
  error?: string
}

export const SettingsSecretInput = forwardRef<HTMLInputElement, SettingsSecretInputProps>(
  ({ label, error, className, ...props }, ref) => {
    const [visible, setVisible] = useState(false)

    return (
      <div className="space-y-1.5">
        {label && (
          <label className="text-sm font-medium text-foreground">{label}</label>
        )}
        <div className="relative">
          <Input
            ref={ref}
            type={visible ? 'text' : 'password'}
            className={cn(
              'h-9 text-sm pr-16',
              error && 'border-destructive',
              className
            )}
            {...props}
          />
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="absolute right-1 top-1/2 -translate-y-1/2 h-7 px-2 text-xs"
            onClick={() => setVisible(!visible)}
          >
            {visible ? '隐藏' : '显示'}
          </Button>
        </div>
        {error && (
          <p className="text-xs text-destructive">{error}</p>
        )}
      </div>
    )
  }
)

SettingsSecretInput.displayName = 'SettingsSecretInput'
