// WeChat iLink QR-binding panel — presentation only. The QR fetch / poll loop /
// token save / disconnect state machine + IPC live in `useWechatIlinkBinding`
// (no direct Tauri IPC here). Only the canvas draw effect stays local since it
// touches the DOM ref. Moved out of `legacy settings/` during the IM-channel
// migration; behavior preserved verbatim.
import { useEffect, useRef } from 'react'
import QRCode from 'qrcode'
import type { ImChannelStatus } from '@/atoms/im-channel-atoms'
import { useWechatIlinkBinding } from '../../hooks/useWechatIlinkBinding'

interface Props {
  instanceId: string
  accountId?: string
  status: ImChannelStatus | undefined
  onSaved: () => void
  onDisconnect: () => void
}

export function WechatIlinkBindingPanel({
  instanceId, accountId, status, onSaved, onDisconnect,
}: Props) {
  const { bindState, fetchQr, cancelToIdle, resetToIdle, handleDisconnect } =
    useWechatIlinkBinding({ instanceId, accountId, status, onSaved, onDisconnect })
  const canvasRef = useRef<HTMLCanvasElement>(null)

  // Render QR canvas using qrcodeImgContent (the URL/token WeChat understands),
  // not qrcode (the polling token).
  useEffect(() => {
    if (
      (bindState.kind === 'qr-shown' || bindState.kind === 'scanning') &&
      canvasRef.current
    ) {
      QRCode.toCanvas(canvasRef.current, bindState.qrcodeImgContent, { width: 128 }).catch(() => {})
    }
  }, [bindState])

  if (bindState.kind === 'idle') {
    return (
      <div className="flex flex-col items-center gap-3 py-4">
        <p className="text-xs text-muted-foreground text-center">
          扫描二维码将此渠道与您的微信账号绑定，即可收发消息
        </p>
        <button
          type="button"
          onClick={fetchQr}
          className="rounded bg-primary px-4 py-2 text-sm text-primary-foreground"
        >
          获取二维码
        </button>
      </div>
    )
  }

  if (bindState.kind === 'loading') {
    return (
      <div className="flex items-center justify-center py-8">
        <span className="text-sm text-muted-foreground">正在获取二维码…</span>
      </div>
    )
  }

  if (bindState.kind === 'qr-shown' || bindState.kind === 'scanning') {
    return (
      <div className="flex flex-col items-center gap-2 py-3">
        <canvas ref={canvasRef} width={128} height={128} className="rounded border border-border" />
        <p className="text-xs text-muted-foreground">
          {bindState.kind === 'scanning' ? '已扫码，等待确认…' : '用微信扫码绑定账号'}
        </p>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={fetchQr}
            className="text-xs text-muted-foreground hover:underline"
          >
            刷新二维码
          </button>
          <span className="text-xs text-muted-foreground">·</span>
          <button
            type="button"
            onClick={cancelToIdle}
            className="text-xs text-muted-foreground hover:underline"
          >
            取消
          </button>
        </div>
      </div>
    )
  }

  if (bindState.kind === 'qr-expired') {
    return (
      <div className="flex flex-col items-center gap-2 py-4">
        <p className="text-sm text-amber-500">二维码已过期</p>
        <button
          type="button"
          onClick={fetchQr}
          className="rounded bg-primary px-4 py-2 text-sm text-primary-foreground"
        >
          重新获取
        </button>
      </div>
    )
  }

  if (bindState.kind === 'error') {
    return (
      <div className="flex flex-col items-center gap-2 py-4">
        <p className="text-sm text-destructive text-center">{bindState.message}</p>
        <button
          type="button"
          onClick={resetToIdle}
          className="text-xs text-muted-foreground hover:underline"
        >
          重试
        </button>
      </div>
    )
  }

  // confirmed
  return (
    <div className="rounded border border-success/30 bg-success/5 p-3 space-y-2">
      <div className="flex items-center gap-2">
        <span className="w-2 h-2 rounded-full bg-success flex-shrink-0" />
        <span className="text-xs font-medium text-success">已绑定</span>
      </div>
      {accountId && (
        <p className="text-xs text-muted-foreground">账号: {accountId}</p>
      )}
      <div className="flex items-center gap-2 pt-1">
        <button
          type="button"
          onClick={fetchQr}
          className="text-xs text-muted-foreground hover:underline"
        >
          重新绑定
        </button>
        <span className="text-xs text-muted-foreground">·</span>
        <button
          type="button"
          onClick={handleDisconnect}
          className="text-xs text-destructive hover:underline"
        >
          断开连接
        </button>
      </div>
    </div>
  )
}
