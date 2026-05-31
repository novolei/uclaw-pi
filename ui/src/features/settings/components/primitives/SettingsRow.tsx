import * as React from 'react'
import { cn } from '@/lib/utils'
import { LABEL_CLASS, DESCRIPTION_CLASS, ROW_CLASS } from './SettingsUIConstants'

interface SettingsRowProps {
  label: string
  icon?: React.ReactNode
  description?: string
  children?: React.ReactNode
  className?: string
}

export function SettingsRow({ label, icon, description, children, className }: SettingsRowProps) {
  return (
    <div className={cn(ROW_CLASS, className)}>
      {icon && <div className="flex-shrink-0 mr-3">{icon}</div>}
      <div className="flex-1 min-w-0 mr-4">
        <div className={LABEL_CLASS}>{label}</div>
        {description && (
          <div className={cn(DESCRIPTION_CLASS, 'mt-0.5')}>{description}</div>
        )}
      </div>
      {children && <div className="flex-shrink-0">{children}</div>}
    </div>
  )
}
