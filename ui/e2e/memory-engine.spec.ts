import { test, expect, type Page } from '@playwright/test'

/**
 * Settings › 记忆 — MemoryEngineCard against `npm run dev:mock-tauri`.
 *
 * `?memoryCore=downloading` starts the mock mid-download; `dev-memory-core-mock.ts`
 * streams `memory-core:status` events through the rest of the download, loads
 * the models and fills the vector index (~15 s). The flows assert on the copy
 * and ARIA names the card renders — no test-only hooks.
 */

test.describe.configure({ timeout: 60_000 })

/** The header's 模式 StatPill (avoids matching the same words elsewhere). */
const modePill = (page: Page) => page.locator('[data-tone]', { hasText: '模式' })

async function openMemorySettings(page: Page, state: string): Promise<void> {
  await page.addInitScript(() => {
    try {
      localStorage.setItem('uclaw.onboarding.complete', '1')
    } catch {
      /* ignore */
    }
  })
  await page.goto(`/?memoryCore=${state}`)
  await page.getByRole('button', { name: /Mock User/ }).first().click()
  await page.getByRole('button', { name: /^记忆( |$)/ }).first().click()
  await expect(page.getByRole('switch', { name: '使用本地检索模型' })).toBeVisible()
}

test.describe('Settings › 记忆 engine card (dev:mock-tauri)', () => {
  test('downloading → ready: progress streams, then both models ready and hybrid mode', async ({ page }) => {
    await openMemorySettings(page, 'downloading')

    // Mid-download: named progress bar, byte counts as text, keyword-only mode.
    const bar = page.getByRole('progressbar', { name: '下载记忆模型' })
    await expect(page.getByText('正在下载模型')).toBeVisible()
    await expect(bar).toBeVisible()
    await expect(page.getByText(/MB \/ 1\.14 GB/)).toBeVisible()
    await expect(modePill(page)).toContainText('仅关键词')

    // Progress advances as status events arrive.
    const first = Number(await bar.getAttribute('aria-valuenow'))
    await expect
      .poll(async () => Number(await bar.getAttribute('aria-valuenow')), { timeout: 10_000 })
      .toBeGreaterThan(first)

    // Download done → models load → hybrid mode, both models ready.
    await expect(modePill(page)).toContainText('混合检索', { timeout: 40_000 })
    await expect(bar).toHaveCount(0)
    const models = page.getByRole('list', { name: '本地模型' })
    await expect(models.getByText('已就绪')).toHaveCount(2)

    // The index then fills to full semantic coverage.
    await expect(page.getByRole('progressbar', { name: '语义覆盖' })).toHaveAttribute('aria-valuenow', '100', {
      timeout: 15_000,
    })
  })

  test('toggling the models off switches to keyword-only copy', async ({ page }) => {
    await openMemorySettings(page, 'ready')
    await expect(modePill(page)).toContainText('混合检索')

    const toggle = page.getByRole('switch', { name: '使用本地检索模型' })
    await expect(toggle).toBeChecked()
    await toggle.click()

    await expect(toggle).not.toBeChecked()
    await expect(modePill(page)).toContainText('仅关键词')
    await expect(page.getByText('当前仅按关键词找回记忆')).toBeVisible()
    // The models stay on disk, so turning them back on needs no download.
    await expect(page.getByText(/开启后立即可用，模型已在本机/)).toBeVisible()
    await expect(page.getByText(/首次开启会下载/)).toHaveCount(0)
  })

  test('off with no models on disk promises the first download', async ({ page }) => {
    await openMemorySettings(page, 'off')
    await expect(page.getByText(/首次开启会下载约 1\.1 GB 模型/)).toBeVisible()
  })

  test('a failed model load shows a callout and 重试 reloads to hybrid', async ({ page }) => {
    await openMemorySettings(page, 'loadfailed')
    const alert = page.getByRole('alert')
    await expect(alert.getByText('模型加载失败')).toBeVisible()
    await expect(page.getByText('正在加载模型')).toHaveCount(0)
    await alert.getByRole('button', { name: '重试' }).click()
    await expect(modePill(page)).toContainText('混合检索', { timeout: 15_000 })
    await expect(page.getByRole('list', { name: '本地模型' }).getByText('已就绪')).toHaveCount(2)
  })
})
