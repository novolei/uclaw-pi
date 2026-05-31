import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, waitFor } from '@/test-utils/render'
import { SkillSuggestionBar } from './SkillSuggestionBar'

// The hook loads the skill catalog through the agent bridge; stub it so the
// render test never reaches `@tauri-apps/api`.
const listSkills = vi.fn()
const listLearnedSkills = vi.fn()
vi.mock('@/lib/bridge/agent', () => ({
  listSkills: (...a: unknown[]) => listSkills(...a),
  listLearnedSkills: (...a: unknown[]) => listLearnedSkills(...a),
}))

describe('SkillSuggestionBar', () => {
  it('renders nothing for a short query (no search fired)', () => {
    listSkills.mockResolvedValue([])
    listLearnedSkills.mockResolvedValue([])
    const { container } = renderWithProviders(
      <SkillSuggestionBar inputText="hi" onSkillSelect={vi.fn()} />,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders a matching suggestion chip after the debounce', async () => {
    listSkills.mockResolvedValue([
      { name: 'stock-research', description: 'research stocks', category: '', enabled: true },
    ])
    listLearnedSkills.mockResolvedValue([])
    const { findByText } = renderWithProviders(
      <SkillSuggestionBar inputText="stock-research" onSkillSelect={vi.fn()} />,
    )
    // Debounce is 500ms; findByText polls up to the default timeout.
    expect(await findByText('stock-research')).toBeTruthy()
  })

  it('invokes onSkillSelect with a slash-prefixed name on click', async () => {
    listSkills.mockResolvedValue([
      { name: 'stock-research', description: 'research stocks', category: '', enabled: true },
    ])
    listLearnedSkills.mockResolvedValue([])
    const onSkillSelect = vi.fn()
    const { findByText, user } = renderWithProviders(
      <SkillSuggestionBar inputText="stock-research" onSkillSelect={onSkillSelect} />,
    )
    const chip = await findByText('stock-research')
    await user.click(chip)
    await waitFor(() => expect(onSkillSelect).toHaveBeenCalledWith('/stock-research'))
  })
})
