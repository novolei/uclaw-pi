import { useState } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { Button } from '@/components/ui/button'
import { checkForUpdates, installAndRelaunch } from '@/atoms/updater'

export function UpdateDialog() {
  const [checking, setChecking] = useState(false)
  const [updateAvailable, setUpdateAvailable] = useState(false)
  const [updateInfo, setUpdateInfo] = useState<{ version: string; body: string } | null>(null)
  const [installing, setInstalling] = useState(false)
  const [downloaded, setDownloaded] = useState(0)
  const [contentLength, setContentLength] = useState<number | undefined>(undefined)

  const handleCheckForUpdates = async () => {
    setChecking(true)
    try {
      const update = await checkForUpdates()
      if (update) {
        setUpdateAvailable(true)
        setUpdateInfo(update)
      } else {
        setUpdateAvailable(false)
        setUpdateInfo(null)
      }
    } finally {
      setChecking(false)
    }
  }

  const handleInstallAndRelaunch = async () => {
    setInstalling(true)
    setDownloaded(0)
    setContentLength(undefined)
    try {
      await installAndRelaunch((dl, total) => {
        setDownloaded((prev) => prev + dl)
        if (total !== undefined) setContentLength(total)
      })
    } finally {
      setInstalling(false)
    }
  }

  const progressPercent =
    installing && contentLength && contentLength > 0
      ? Math.min(100, Math.round((downloaded / contentLength) * 100))
      : null

  const formatBytes = (bytes: number) => {
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">检查更新</h2>

      <SettingsSection>
        <SettingsCard>
          <div className="flex flex-col items-center py-6 space-y-4">
            {updateAvailable && updateInfo ? (
              <>
                <p className="text-sm">发现新版本 {updateInfo.version}，是否更新？</p>
                {updateInfo.body && (
                  <p className="text-xs text-muted-foreground max-w-xs text-center">
                    {updateInfo.body}
                  </p>
                )}

                {installing && (
                  <div className="w-full max-w-xs space-y-1">
                    <div className="h-2 rounded-full bg-muted overflow-hidden">
                      <div
                        className="h-full bg-primary transition-all duration-200"
                        style={{ width: progressPercent !== null ? `${progressPercent}%` : '0%' }}
                      />
                    </div>
                    <p className="text-xs text-muted-foreground text-center">
                      {progressPercent !== null
                        ? `${progressPercent}% — ${formatBytes(downloaded)} / ${formatBytes(contentLength!)}`
                        : `${formatBytes(downloaded)} 已下载...`}
                    </p>
                  </div>
                )}

                <Button onClick={handleInstallAndRelaunch} disabled={installing}>
                  {installing ? '下载中...' : '下载并安装'}
                </Button>
              </>
            ) : (
              <>
                <p className="text-sm text-muted-foreground">
                  {checking ? '正在检查更新...' : '当前已是最新版本'}
                </p>
                <Button variant="outline" onClick={handleCheckForUpdates} disabled={checking}>
                  {checking ? '检查中...' : '检查更新'}
                </Button>
              </>
            )}
          </div>
        </SettingsCard>
      </SettingsSection>
    </div>
  )
}
