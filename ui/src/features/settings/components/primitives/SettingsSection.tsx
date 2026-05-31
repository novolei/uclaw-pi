import * as React from 'react'
import { SECTION_TITLE_CLASS, SECTION_DESCRIPTION_CLASS } from './SettingsUIConstants'

interface SettingsSectionProps {
  title?: React.ReactNode
  description?: string
  action?: React.ReactNode
  children: React.ReactNode
  className?: string
}

export function SettingsSection({ title, description, action, children, className }: SettingsSectionProps) {
  return (
    <div className={`space-y-3 ${className ?? ''}`}>
      {(title || description) && (
        <div className="flex items-start justify-between">
          <div>
            {title && <h4 className={SECTION_TITLE_CLASS}>{title}</h4>}
            {description && <p className={SECTION_DESCRIPTION_CLASS}>{description}</p>}
          </div>
          {action && <div className="flex-shrink-0 ml-4">{action}</div>}
        </div>
      )}
      {children}
    </div>
  )
}
