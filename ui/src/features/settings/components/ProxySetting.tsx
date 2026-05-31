import { useState } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsInput } from './primitives/SettingsInput'
import { SettingsToggle } from './primitives/SettingsToggle'
import { SettingsSelect } from './primitives/SettingsSelect'
import { Button } from '@/components/ui/button'

const PROXY_TYPE_OPTIONS = [
  { value: 'none', label: '无代理' },
  { value: 'http', label: 'HTTP' },
  { value: 'socks5', label: 'SOCKS5' },
  { value: 'system', label: '系统代理' },
]

export function ProxySetting() {
  const [proxyType, setProxyType] = useState('none')
  const [host, setHost] = useState('')
  const [port, setPort] = useState('')
  const [authEnabled, setAuthEnabled] = useState(false)
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')

  const handleSave = () => {
    // [PLACEHOLDER - Tauri adaptation needed] Save proxy settings
    console.log('Save proxy settings:', { proxyType, host, port, authEnabled, username })
  }

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">代理设置</h2>

      <SettingsSection title="代理配置" description="配置网络代理用于 API 请求">
        <SettingsSelect
          label="代理类型"
          value={proxyType}
          onValueChange={setProxyType}
          options={PROXY_TYPE_OPTIONS}
        />

        {(proxyType === 'http' || proxyType === 'socks5') && (
          <>
            <div className="grid grid-cols-[1fr_120px] gap-2">
              <SettingsInput
                label="主机"
                value={host}
                onChange={(e) => setHost(e.target.value)}
                placeholder="127.0.0.1"
              />
              <SettingsInput
                label="端口"
                value={port}
                onChange={(e) => setPort(e.target.value)}
                placeholder="7890"
              />
            </div>

            <SettingsToggle
              label="需要认证"
              checked={authEnabled}
              onCheckedChange={setAuthEnabled}
            />

            {authEnabled && (
              <>
                <SettingsInput
                  label="用户名"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                />
                <SettingsInput
                  label="密码"
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                />
              </>
            )}
          </>
        )}

        <Button size="sm" onClick={handleSave}>
          保存代理设置
        </Button>
      </SettingsSection>
    </div>
  )
}
