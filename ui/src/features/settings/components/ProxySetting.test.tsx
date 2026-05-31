import { describe, it, expect } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { ProxySetting } from './ProxySetting'

// Pure local-state form (no IPC; handleSave is a [PLACEHOLDER] console.log). The
// host/port/auth fields only appear once the proxy type is http/socks5 — at the
// default 'none' they stay hidden, so the smoke test asserts the always-present
// chrome (heading, type select, save button).
describe('ProxySetting', () => {
  it('renders the 代理设置 page with the type select + save button', () => {
    renderWithProviders(<ProxySetting />)
    expect(screen.getByText('代理设置')).not.toBeNull()
    expect(screen.getByText('代理类型')).not.toBeNull()
    expect(screen.getByText('保存代理设置')).not.toBeNull()
  })
})
