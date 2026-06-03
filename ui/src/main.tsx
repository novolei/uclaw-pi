/**
 * 渲染进程入口
 *
 * 挂载 React 应用，初始化主题系统。
 *
 * [Migration] 从 Proma main.tsx 迁移：
 * - 移除 Electron quick-task 窗口检测
 * - 移除 Electron IPC 初始化器（Agent、Chat、Feishu、DingTalk 等）
 * - 保留 ThemeInitializer 核心逻辑
 * - 其余初始化器改为占位，待后续任务逐步接入 Tauri 后端
 */

// Browser-only UI debugging must install the mock before any module imports
// `@tauri-apps/api` or `tauri-bridge`.
import './lib/dev-tauri-mock'
import React, { useEffect, useMemo } from 'react'
import ReactDOM from 'react-dom/client'
import { useSetAtom, useAtomValue } from 'jotai'
import App from './App'
import {
  themeModeAtom,
  themeStyleAtom,
  systemIsDarkAtom,
  resolvedThemeAtom,
  applyThemeToDOM,
  initializeTheme,
} from './atoms/theme'
import { Toaster } from './components/ui/sonner'
import { GlobalShortcuts } from './components/shortcuts/GlobalShortcuts'
import { AutomationLoginBrowserWindow } from './components/automation/AutomationLoginBrowserWindow'
import { DeskPetApp } from './components/deskpet/DeskPetApp'
import './styles/globals.css'

// App-wide right-click bridge — ensures the contextmenu event fires for
// every right-button mousedown, working around macOS WKWebView setups
// where Radix ContextMenu triggers otherwise stay silent. Installs once
// at module load (idempotent).
import { installRightClickBridge } from './lib/right-click-bridge'
installRightClickBridge()

// 开机即应用聊天外观（避免首次渲染时闪烁）
import { applyChatAppearanceToDOM } from './atoms/chat-appearance'
try {
  const fontSize = (localStorage.getItem('uclaw-chat-font-size') as 'sm' | 'md' | 'lg' | null) ?? 'md'
  const serif = localStorage.getItem('uclaw-chat-serif') === 'true'
  applyChatAppearanceToDOM(
    fontSize === 'sm' || fontSize === 'md' || fontSize === 'lg' ? fontSize : 'md',
    serif,
  )
} catch {
  /* ignore */
}

// 导入 tauri-bridge 以注册 IPC 适配层
import './lib/tauri-bridge'
import { installUclawDebugBridge } from './lib/uclaw-debug-bridge'
installUclawDebugBridge()

/**
 * 主题初始化组件
 *
 * 负责从后端加载主题设置、监听系统主题变化、
 * 并将最终主题同步到 DOM。
 */
function ThemeInitializer(): null {
  const setThemeMode = useSetAtom(themeModeAtom)
  const setThemeStyle = useSetAtom(themeStyleAtom)
  const setSystemIsDark = useSetAtom(systemIsDarkAtom)
  const themeMode = useAtomValue(themeModeAtom)
  const themeStyle = useAtomValue(themeStyleAtom)
  const systemIsDark = useAtomValue(systemIsDarkAtom)

  // 初始化：从后端加载设置 + 订阅系统主题变化
  useEffect(() => {
    let isMounted = true
    let cleanup: (() => void) | undefined

    initializeTheme(setThemeMode, setSystemIsDark, setThemeStyle).then((fn) => {
      if (isMounted) {
        cleanup = fn
      } else {
        fn()
      }
    }).catch((err) => {
      console.warn('[ThemeInitializer] 主题初始化失败（Tauri API 可能不可用）:', err)
    })

    return () => {
      isMounted = false
      cleanup?.()
    }
  }, [setThemeMode, setSystemIsDark, setThemeStyle])

  // 响应式应用主题到 DOM
  const themeSignature = useMemo(() => {
    if (themeMode === 'special') return `special:${themeStyle}`
    if (themeMode === 'system') return `system:${systemIsDark ? 'dark' : 'light'}`
    return themeMode
  }, [themeMode, themeStyle, systemIsDark])

  useEffect(() => {
    applyThemeToDOM(themeMode, themeStyle, systemIsDark)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [themeSignature])

  return null
}

// ===== ErrorBoundary =====
class RootErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { hasError: boolean; error: Error | null }
> {
  constructor(props: { children: React.ReactNode }) {
    super(props)
    this.state = { hasError: false, error: null }
  }
  static getDerivedStateFromError(error: Error) {
    return { hasError: true, error }
  }
  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error('[RootErrorBoundary] 渲染错误:', error, info.componentStack)
  }
  render() {
    if (this.state.hasError) {
      return (
        <div style={{ padding: 32, fontFamily: 'system-ui', color: '#e5e5e5', background: '#121212', minHeight: '100vh' }}>
          <h2 style={{ color: '#f87171' }}>uClaw 渲染错误</h2>
          <pre style={{ whiteSpace: 'pre-wrap', fontSize: 13, marginTop: 16, padding: 16, background: '#1e1e1e', borderRadius: 8 }}>
            {this.state.error?.message}\n{this.state.error?.stack}
          </pre>
        </div>
      )
    }
    return this.props.children
  }
}

const searchParams = new URLSearchParams(window.location.search)
const isAutomationLoginBrowserWindow =
  searchParams.get('uclawWindow') === 'automation-login-browser'
// S4: the frameless transparent desk-pet window loads `index.html?view=deskpet`
// and renders a LIGHT root (no AppShell / agent loop), so OS transparency shows.
const isDeskPetWindow = searchParams.get('view') === 'deskpet'

if (isDeskPetWindow) {
  // Make the document chrome transparent so the OS window transparency is visible
  // (the deskpet root paints `background: transparent`).
  document.documentElement.style.background = 'transparent'
  document.body.style.background = 'transparent'
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <RootErrorBoundary>
      <ThemeInitializer />
      {isDeskPetWindow ? (
        <DeskPetApp />
      ) : isAutomationLoginBrowserWindow ? (
        <>
          <AutomationLoginBrowserWindow />
          <Toaster />
        </>
      ) : (
        <>
          <GlobalShortcuts />
          <App />
          <Toaster />
        </>
      )}
    </RootErrorBoundary>
  </React.StrictMode>
)
