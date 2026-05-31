// Owns the WeChat iLink QR-binding state machine — QR fetch, the 2s poll loop
// (scaned/confirmed/expired transitions), token save, and disconnect. Extracted
// out of `WechatIlinkBindingPanel` during the IM-channel migration. All IPC goes
// through `settingsBridge` (no `@tauri-apps/api` here). The mutually-recursive
// saveToken ↔ startPolling ref dance, the 120s expiry, and the canvas content
// (qrcodeImgContent vs the polling qrcode token) are preserved verbatim.
import { useCallback, useEffect, useRef, useState } from 'react'
import { toast } from 'sonner'
import type { ImChannelStatus } from '@/atoms/im-channel-atoms'
import { settingsBridge } from '../../../lib/bridge/settings'

export type BindState =
  | { kind: 'idle' }
  | { kind: 'loading' }
  // qrcode = polling token; qrcodeImgContent = value encoded into the QR image
  | { kind: 'qr-shown'; qrcode: string; qrcodeImgContent: string }
  | { kind: 'scanning'; qrcode: string; qrcodeImgContent: string }
  | { kind: 'confirmed' }
  | { kind: 'qr-expired' }
  | { kind: 'error'; message: string }

interface Args {
  instanceId: string
  accountId?: string
  status: ImChannelStatus | undefined
  onSaved: () => void
  onDisconnect: () => void
}

export function useWechatIlinkBinding({
  instanceId,
  accountId,
  status,
  onSaved,
  onDisconnect,
}: Args) {
  const [bindState, setBindState] = useState<BindState>(
    accountId ? { kind: 'confirmed' } : { kind: 'idle' },
  )
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const pollStartRef = useRef<number>(0)

  const stopPolling = useCallback(() => {
    if (pollRef.current !== null) {
      clearInterval(pollRef.current)
      pollRef.current = null
    }
  }, [])

  useEffect(() => () => { stopPolling() }, [stopPolling])

  // saveToken and startPolling are mutually recursive; use refs so each
  // useCallback can see the latest version of the other without listing it
  // as a dependency (avoiding an infinite dep cycle).
  const saveTokenRef = useRef<(botToken: string, accId: string, qrcode: string, qrcodeImgContent: string) => Promise<void>>(
    async () => {},
  )
  const startPollingRef = useRef<(qrcode: string, qrcodeImgContent: string) => void>(() => {})

  const startPolling = useCallback((qrcode: string, qrcodeImgContent: string) => {
    stopPolling()
    pollStartRef.current = Date.now()
    pollRef.current = setInterval(async () => {
      if (Date.now() - pollStartRef.current > 120_000) {
        stopPolling()
        setBindState({ kind: 'qr-expired' })
        return
      }
      try {
        const result = await settingsBridge.pollWechatIlinkQrcodeStatus(instanceId, qrcode)

        if (result.status === 'scaned') {
          setBindState({ kind: 'scanning', qrcode, qrcodeImgContent })
        } else if (result.status === 'confirmed' && result.bot_token && result.account_id) {
          stopPolling()
          await saveTokenRef.current(result.bot_token, result.account_id, qrcode, qrcodeImgContent)
        } else if (result.status === 'expired') {
          stopPolling()
          setBindState({ kind: 'qr-expired' })
        }
      } catch {
        // Network error during poll — keep retrying
      }
    }, 2000)
  }, [instanceId, stopPolling])

  const saveToken = useCallback(async (botToken: string, accId: string, qrcode: string, qrcodeImgContent: string) => {
    try {
      await settingsBridge.saveWechatIlinkToken(instanceId, botToken, accId)
      setBindState({ kind: 'confirmed' })
      onSaved()
    } catch (e) {
      toast.error('保存绑定信息失败：' + String(e))
      setBindState({ kind: 'qr-shown', qrcode, qrcodeImgContent })
      startPollingRef.current(qrcode, qrcodeImgContent)
    }
  }, [instanceId, onSaved])

  // Keep refs in sync so the interval callbacks always call the latest version
  useEffect(() => { startPollingRef.current = startPolling }, [startPolling])
  useEffect(() => { saveTokenRef.current = saveToken }, [saveToken])

  const fetchQr = useCallback(async () => {
    stopPolling()
    setBindState({ kind: 'loading' })
    try {
      const result = await settingsBridge.requestWechatIlinkQrcode(instanceId)
      setBindState({ kind: 'qr-shown', qrcode: result.qrcode, qrcodeImgContent: result.qrcode_img_content })
      startPolling(result.qrcode, result.qrcode_img_content)
    } catch (e) {
      setBindState({ kind: 'error', message: String(e) })
    }
  }, [instanceId, stopPolling, startPolling])

  // Auto-trigger QR fetch on iLink session expiry (-14)
  useEffect(() => {
    if (status?.state === 'needs_rebind') {
      fetchQr()
    }
  }, [status?.state, fetchQr])

  const cancelToIdle = useCallback(() => {
    stopPolling()
    setBindState({ kind: 'idle' })
  }, [stopPolling])

  const resetToIdle = useCallback(() => setBindState({ kind: 'idle' }), [])

  const handleDisconnect = useCallback(async () => {
    stopPolling()
    try {
      await settingsBridge.disconnectWechatIlink(instanceId)
      setBindState({ kind: 'idle' })
      onDisconnect()
    } catch (e) {
      toast.error('断开失败：' + String(e))
    }
  }, [instanceId, stopPolling, onDisconnect])

  return { bindState, fetchQr, cancelToIdle, resetToIdle, handleDisconnect }
}
