// Shared prop shape for the MemoryRecall section cards: the current config + the
// field-update callback (both owned by useMemoryRecallSettings).
import type { MemoryRecallConfigDto } from '@/lib/tauri-bridge'

export interface MemoryRecallCardProps {
  config: MemoryRecallConfigDto
  updateField: <K extends keyof MemoryRecallConfigDto>(
    key: K,
    value: MemoryRecallConfigDto[K],
  ) => void
}
