import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'

interface SettingsSelectOption {
  value: string
  label: string
}

interface SettingsSelectProps {
  value: string
  onValueChange: (value: string) => void
  options: SettingsSelectOption[]
  placeholder?: string
  label?: string
  className?: string
}

export function SettingsSelect({
  value,
  onValueChange,
  options,
  placeholder = '请选择...',
  label,
  className,
}: SettingsSelectProps) {
  return (
    <div className="space-y-1.5">
      {label && (
        <label className="text-sm font-medium text-foreground">{label}</label>
      )}
      <Select value={value} onValueChange={onValueChange}>
        <SelectTrigger className={className}>
          <SelectValue placeholder={placeholder} />
        </SelectTrigger>
        <SelectContent>
          {options
            .filter((opt) => opt.value !== '')
            .map((opt) => (
              <SelectItem key={opt.value} value={opt.value}>
                {opt.label}
              </SelectItem>
            ))}
        </SelectContent>
      </Select>
    </div>
  )
}
