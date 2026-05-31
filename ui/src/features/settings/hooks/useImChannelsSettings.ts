// Owns the ImChannelsSettings data + side effects: the channels/statuses atoms,
// the spaces fetch, the realtime im_channel_status_changed subscription (via the
// settingsBridge.onImChannelStatusChanged wrapper — no direct Tauri IPC here),
// and the toggle/save/delete handlers. Extracted out of ImChannelsSettings
// during the migration. The optimistic toggle + revert-on-failure and the
// delete confirm() are preserved verbatim. Pure-UI state (active tab, open row,
// adding-to-type) stays in the component.
import { useAtom, useSetAtom } from 'jotai'
import { useEffect, useState } from 'react'
import { toast } from 'sonner'
import {
  imChannelsAtom,
  fetchImChannelsAtom,
  imChannelStatusesAtom,
  fetchImChannelStatusesAtom,
} from '@/atoms/im-channel-atoms'
import { settingsBridge, onImChannelStatusChanged } from '../../../lib/bridge/settings'

export function useImChannelsSettings() {
  const [channels, setChannels] = useAtom(imChannelsAtom)
  const fetchChannels = useSetAtom(fetchImChannelsAtom)
  const [statuses, setStatuses] = useAtom(imChannelStatusesAtom)
  const fetchStatuses = useSetAtom(fetchImChannelStatusesAtom)
  const [spaces, setSpaces] = useState<{ id: string; name: string }[]>([])

  useEffect(() => {
    fetchChannels()
    fetchStatuses()
    settingsBridge
      .listSpaces()
      .then(rows => setSpaces(rows.map(s => ({ id: s.id, name: s.name }))))
      .catch(() => {})
  }, [fetchChannels, fetchStatuses])

  // Realtime status updates from backend
  useEffect(() => {
    const unlisten = onImChannelStatusChanged((payload) => {
      setStatuses(prev => ({ ...prev, [payload.instanceId]: payload }))
    })
    return () => { unlisten.then(fn => fn()) }
  }, [setStatuses])

  async function handleToggle(id: string, enabled: boolean) {
    setChannels(prev => prev.map(ch => ch.id === id ? { ...ch, enabled } : ch))
    try {
      await settingsBridge.toggleImChannel(id, enabled)
    } catch (e) {
      fetchChannels()
      toast.error('切换失败：' + String(e))
    }
  }

  function handleSaved() {
    fetchChannels()
    fetchStatuses()
  }

  async function handleDelete(id: string) {
    if (!confirm('确定删除此渠道实例？')) return
    try {
      await settingsBridge.deleteImChannel(id)
      fetchChannels()
    } catch (e) {
      toast.error('删除失败：' + String(e))
    }
  }

  return { channels, statuses, spaces, handleToggle, handleSaved, handleDelete }
}
