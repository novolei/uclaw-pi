import { describe, it, expect, vi } from 'vitest'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { SettingsNav } from './SettingsNav'

describe('SettingsNav', () => {
  it('renders the tabs grouped under 3 group headers', () => {
    renderWithProviders(
      <SettingsNav
        active="connectivity"
        onChange={() => {}}
        hasUpdate={false}
        sttNeedsDownload={false}
      />,
    )
    // Group headers
    expect(screen.getByText('核心')).not.toBeNull()
    expect(screen.getByText('偏好')).not.toBeNull()
    expect(screen.getByText('系统')).not.toBeNull()
    // Sample tabs from each group
    expect(screen.getByText('服务商与用量')).not.toBeNull()
    expect(screen.getByText('智能')).not.toBeNull()
    expect(screen.getByText('工具与能力')).not.toBeNull()
    expect(screen.getByText('学到的偏好')).not.toBeNull()
    expect(screen.getByText('通用与外观')).not.toBeNull()
    expect(screen.getByText('输入（语音）')).not.toBeNull()
    expect(screen.getByText('代理')).not.toBeNull()
    expect(screen.getByText('浏览器运行时')).not.toBeNull()
    expect(screen.getByText('关于')).not.toBeNull()
  })

  it('clicking a tab calls onChange with its id', () => {
    const onChange = vi.fn()
    renderWithProviders(
      <SettingsNav
        active="connectivity"
        onChange={onChange}
        hasUpdate={false}
        sttNeedsDownload={false}
      />,
    )
    fireEvent.click(screen.getByText('智能'))
    expect(onChange).toHaveBeenCalledWith('intelligence')
  })

  it('search filters tabs (case-insensitive substring on label)', () => {
    renderWithProviders(
      <SettingsNav
        active="connectivity"
        onChange={() => {}}
        hasUpdate={false}
        sttNeedsDownload={false}
      />,
    )
    const search = screen.getByPlaceholderText(/搜索/)
    fireEvent.change(search, { target: { value: '语音' } })
    const inputTab = screen.getByText('输入（语音）').closest('button')
    expect(inputTab?.className).not.toContain('opacity-40')
    const proxyTab = screen.getByText('代理').closest('button')
    expect(proxyTab?.className).toContain('opacity-40')
  })

  it('about shows red dot when hasUpdate=true', () => {
    renderWithProviders(
      <SettingsNav
        active="about"
        onChange={() => {}}
        hasUpdate={true}
        sttNeedsDownload={false}
      />,
    )
    const aboutBtn = screen.getByText('关于').closest('button')
    expect(aboutBtn?.querySelector('[data-update-dot]')).not.toBeNull()
  })

  it('stt tab shows red dot when sttNeedsDownload=true', () => {
    renderWithProviders(
      <SettingsNav
        active="connectivity"
        onChange={() => {}}
        hasUpdate={false}
        sttNeedsDownload={true}
      />,
    )
    const sttBtn = screen.getByText('输入（语音）').closest('button')
    expect(sttBtn?.querySelector('[data-stt-dot]')).not.toBeNull()
  })
})
