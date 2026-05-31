import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { act, waitFor } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import type { ImChannelStatus } from '@/atoms/im-channel-atoms'
import { WechatIlinkBindingPanel } from './WechatIlinkBindingPanel'

// IPC now flows through settingsBridge (was a raw Tauri-core invoke mock before
// the migration). Each command maps to its own bridge method.
vi.mock('../../../../lib/bridge/settings', () => ({
  settingsBridge: {
    requestWechatIlinkQrcode: vi.fn(),
    pollWechatIlinkQrcodeStatus: vi.fn(),
    saveWechatIlinkToken: vi.fn(),
    disconnectWechatIlink: vi.fn(),
  },
}))
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }))
vi.mock('qrcode', () => ({
  default: { toCanvas: vi.fn().mockResolvedValue(undefined) },
}))

import { settingsBridge } from '../../../../lib/bridge/settings'

const requestQr = vi.mocked(settingsBridge.requestWechatIlinkQrcode)
const pollStatus = vi.mocked(settingsBridge.pollWechatIlinkQrcodeStatus)
const saveToken = vi.mocked(settingsBridge.saveWechatIlinkToken)

const PROPS = {
  instanceId: 'inst-1',
  onSaved: vi.fn(),
  onDisconnect: vi.fn(),
}

beforeEach(() => {
  requestQr.mockReset()
  pollStatus.mockReset()
  saveToken.mockReset()
  PROPS.onSaved = vi.fn()
  PROPS.onDisconnect = vi.fn()
})

describe('WechatIlinkBindingPanel', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  it('idle: shows get-qr button, no canvas', () => {
    renderWithProviders(
      <WechatIlinkBindingPanel {...PROPS} status={undefined} />
    )
    expect(screen.getByText('获取二维码')).not.toBeNull()
    expect(screen.queryByRole('img')).toBeNull()
  })

  it('qr-shown: fetching QR invokes request command and shows canvas', async () => {
    requestQr.mockResolvedValueOnce({ qrcode: 'mock_qr_data', qrcode_img_content: 'mock_qr_data' })
    renderWithProviders(
      <WechatIlinkBindingPanel {...PROPS} status={undefined} />
    )
    const btn = screen.getByText('获取二维码')
    await act(async () => { btn.click() })
    await waitFor(() =>
      expect(requestQr).toHaveBeenCalledWith('inst-1')
    )
    expect(screen.getByText('用微信扫码绑定账号')).not.toBeNull()
  })

  it('scanning: poll returning scaned shows "已扫码" text', async () => {
    vi.useFakeTimers()
    requestQr.mockResolvedValueOnce({ qrcode: 'qr123', qrcode_img_content: 'qr123' })
    pollStatus
      .mockResolvedValueOnce({ status: 'wait' })                 // first poll
      .mockResolvedValueOnce({ status: 'scaned' })               // second poll → scanning
    renderWithProviders(
      <WechatIlinkBindingPanel {...PROPS} status={undefined} />
    )
    await act(async () => { screen.getByText('获取二维码').click() })
    // fire first interval tick and drain all resulting promises
    await act(async () => { await vi.advanceTimersByTimeAsync(2100) })
    // fire second interval tick and drain all resulting promises
    await act(async () => { await vi.advanceTimersByTimeAsync(2100) })
    expect(screen.getByText('已扫码，等待确认…')).not.toBeNull()
  })

  it('confirmed: poll returning confirmed calls save_wechat_ilink_token and onSaved', async () => {
    vi.useFakeTimers()
    requestQr.mockResolvedValueOnce({ qrcode: 'qr123', qrcode_img_content: 'qr123' })
    pollStatus.mockResolvedValueOnce({ status: 'confirmed', bot_token: 'tok999', account_id: 'acc456' })
    saveToken.mockResolvedValueOnce(undefined)
    renderWithProviders(
      <WechatIlinkBindingPanel {...PROPS} status={undefined} />
    )
    await act(async () => { screen.getByText('获取二维码').click() })
    // fire first interval tick and drain all resulting promises (including saveToken)
    await act(async () => { await vi.advanceTimersByTimeAsync(2100) })
    expect(saveToken).toHaveBeenCalledWith('inst-1', 'tok999', 'acc456')
    expect(PROPS.onSaved).toHaveBeenCalledOnce()
  })
})
