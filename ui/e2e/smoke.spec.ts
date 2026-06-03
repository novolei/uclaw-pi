import { test, expect, type ConsoleMessage, type Page } from '@playwright/test'

/**
 * Browser-level frontend smoke (S6) against `npm run dev:mock-tauri`.
 *
 * The dev server boots the real React/Vite app with `dev-tauri-mock.ts`
 * standing in for the Tauri backend; the S1–S5 local-model / pet / onboarding
 * commands are stubbed there (and `download_local_model` / `pet_chat` emit
 * streamed events). These flows target REAL selectors/text read from the
 * components — no contract is invented here.
 */

/** Collect console errors, ignoring the expected mock + dev noise. */
function collectConsoleErrors(page: Page): string[] {
  const errors: string[] = []
  const IGNORE = [
    'unhandled command', // mock default-case warning for un-stubbed commands
    'Download the React DevTools',
    'mock Tauri IPC',
    '[vite]',
    'failed:', // bridge `.catch(...)` warnings (e.g. desk-pet toggle in browser)
    'startDragging unavailable',
  ]
  page.on('console', (msg: ConsoleMessage) => {
    if (msg.type() !== 'error') return
    const text = msg.text()
    if (IGNORE.some((frag) => text.includes(frag))) return
    errors.push(text)
  })
  page.on('pageerror', (err) => errors.push(`pageerror: ${err.message}`))
  return errors
}

test.describe('uClaw frontend smoke (dev:mock-tauri)', () => {
  test('1. app loads without uncaught console errors', async ({ page }) => {
    const errors = collectConsoleErrors(page)
    await page.goto('/')
    // Either the onboarding welcome OR the main shell paints — wait for the
    // app root to have rendered something beyond the startup splash.
    await expect(page.locator('#root')).not.toBeEmpty()
    // Give the startup splash + first paint a beat to settle.
    await page.waitForTimeout(1500)
    expect(errors, `unexpected console errors:\n${errors.join('\n')}`).toEqual([])
  })

  test('2. onboarding gate shows when active model null + flag cleared', async ({ page }) => {
    // The mock returns `get_active_model` -> null; clearing the completion flag
    // forces the first-run onboarding gate (App.tsx isFirstRun).
    await page.addInitScript(() => {
      try {
        localStorage.removeItem('uclaw.onboarding.complete')
        localStorage.removeItem('uclaw.onboarding.localModel')
      } catch {
        /* localStorage unavailable */
      }
    })
    await page.goto('/')

    // Welcome step copy (OnboardingView WelcomeStep).
    await expect(page.getByText('欢迎使用 uClaw')).toBeVisible()

    // Click through to the app: 下一步 advances; on the welcome step "跳过"
    // completes onboarding immediately. Use 跳过 to reach the app shell.
    await page.getByRole('button', { name: '跳过' }).click()

    // The welcome copy should be gone (we left onboarding).
    await expect(page.getByText('欢迎使用 uClaw')).toHaveCount(0)
  })

  test('3. local-model settings: quant selector + download progress', async ({ page }) => {
    // Skip onboarding so we land on the app shell.
    await page.addInitScript(() => {
      try {
        localStorage.setItem('uclaw.onboarding.complete', '1')
      } catch {
        /* ignore */
      }
    })
    await page.goto('/')

    // Open Settings via the bottom dock (aria-label="设置"), then the 智能 tab.
    await openSettingsTab(page, '智能')

    // LocalModelSettings renders: the quant segmented control + the three quants.
    await expect(page.getByText('本地模型（MiniCPM）')).toBeVisible()
    for (const quant of ['Q4_K_M', 'Q8_0', 'F16']) {
      await expect(page.getByText(quant, { exact: true }).first()).toBeVisible()
    }

    // Click download (status === not-downloaded, since is_local_model_present
    // -> false). The mocked progress events should drive the progress UI.
    const downloadBtn = page.getByRole('button', { name: /下载/ }).first()
    await downloadBtn.click()

    // The phase label appears as the streamed events arrive (probing/下载中/校验中).
    await expect(
      page.getByText(/测速中|下载中|校验中|已就绪/).first(),
    ).toBeVisible({ timeout: 10_000 })
  })

  test('4. desk-pet route renders + streamed reply (no crash)', async ({ page }) => {
    const errors = collectConsoleErrors(page)
    await page.goto('/?view=deskpet')

    // The light desk-pet root mounts the sprite (data-testid) + the chat bubble.
    await expect(page.getByTestId('deskpet-sprite')).toBeVisible()
    const input = page.getByPlaceholder('和我聊聊…')
    await expect(input).toBeVisible()

    // Type + send; pet_chat streams pet:reply-delta then pet:reply-done.
    await input.fill('你好')
    await page.getByRole('button', { name: '发送' }).click()

    // The streamed reply balloon should show the mocked text.
    await expect(page.getByText(/你好/).first()).toBeVisible({ timeout: 10_000 })

    expect(errors, `unexpected console errors:\n${errors.join('\n')}`).toEqual([])
  })

  test('5. persona dropdown lists personas including Clawd', async ({ page }) => {
    await page.addInitScript(() => {
      try {
        localStorage.setItem('uclaw.onboarding.complete', '1')
      } catch {
        /* ignore */
      }
    })
    await page.goto('/')

    await openSettingsTab(page, '桌面宠物')

    // The desktop-companion section renders; its "性格" persona select is fed by
    // list_pet_personas (mock -> 5 personas: astro/clawby/clawd/sprout/pixel).
    await expect(page.getByText('桌面伙伴', { exact: true }).first()).toBeVisible()

    // Open the persona combobox (Radix Select, role=combobox) and assert the
    // roster lists Clawd among its options.
    const personaSelect = page.getByRole('combobox').first()
    await expect(personaSelect).toBeVisible({ timeout: 10_000 })
    await personaSelect.click()
    await expect(page.getByRole('option', { name: 'Clawd' })).toBeVisible({ timeout: 10_000 })
  })
})

/**
 * Open the Settings dialog, then activate a tab by its Chinese label.
 *
 * The reliable settings entry on the app shell is the LeftSidebar footer button
 * (it carries the mock user name "Mock User" as its accessible name and opens
 * the settings dialog). The bottom dock's "设置" button is hidden behind a
 * hover region, so it isn't a stable smoke target. Once the dialog is open the
 * left SettingsNav renders one button per tab label.
 */
async function openSettingsTab(page: Page, tabLabel: string): Promise<void> {
  const settingsEntry = page.getByRole('button', { name: /Mock User/ }).first()
  await settingsEntry.click()
  // The settings nav button for the tab (SettingsNav renders label text).
  await page.getByRole('button', { name: tabLabel, exact: true }).first().click()
}
