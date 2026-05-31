import * as React from 'react'
import { Switch } from '@/components/ui/switch'
import { cn } from '@/lib/utils'
import { LABEL_CLASS, DESCRIPTION_CLASS, ROW_CLASS } from './SettingsUIConstants'

interface SettingsToggleProps {
  label: string
  description?: string
  checked: boolean
  onCheckedChange: (checked: boolean) => void
  disabled?: boolean
  className?: string
}

export function SettingsToggle({
  label,
  description,
  checked,
  onCheckedChange,
  disabled = false,
  className,
}: SettingsToggleProps) {
  return (
    <div className={cn(ROW_CLASS, className)}>
      <div className="flex-1 min-w-0 mr-4">
        <div className={LABEL_CLASS}>{label}</div>
        {description && (
          <div className={cn(DESCRIPTION_CLASS, 'mt-0.5')}>{description}</div>
        )}
      </div>
      <Switch checked={checked} onCheckedChange={onCheckedChange} disabled={disabled} />
    </div>
  )
}
