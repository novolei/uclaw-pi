// Owns the MemorySettings boot-node load + the local toggle state. Extracted out
// of the component during the features/settings migration (code-organization ADR
// 2026-05-31): the component renders, the hook owns useState/useEffect/IPC. IPC
// stays in the typed `@/lib/tauri-bridge` memory-graph helper (precedent:
// useChannelSettings keeps its provider IPC there too). Behavior preserved verbatim.
import { useEffect, useState } from 'react'
import { memoryGraphListBoot } from '@/lib/tauri-bridge'

export function useMemorySettings() {
  const [autoMemorize, setAutoMemorize] = useState(true)
  const [graphEnabled, setGraphEnabled] = useState(true)
  const [bootNodes, setBootNodes] = useState<unknown[]>([])

  useEffect(() => {
    memoryGraphListBoot({ limit: 20 })
      .then((data: unknown) => {
        if (Array.isArray(data)) setBootNodes(data)
      })
      .catch(() => {})
  }, [])

  return {
    autoMemorize,
    setAutoMemorize,
    graphEnabled,
    setGraphEnabled,
    bootNodes,
  }
}
