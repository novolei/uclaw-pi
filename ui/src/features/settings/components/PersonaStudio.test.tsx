import { describe, expect, it, vi } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { PersonaStudio } from './PersonaStudio'

vi.mock('@/lib/persona', () => ({
  getPersonaConfig: vi.fn(async () => ({
    presets: [
      {
        id: 'clarity',
        label: 'Clarity',
        role: 'Decision and engineering partner',
        voice: 'Direct',
        profile: {
          presetId: 'clarity',
          warmth: 2,
          directness: 4,
          challenge: 3,
          playfulness: 1,
          detail: 3,
          initiative: 3,
          structure: 4,
          restraint: 4,
          neutralMode: false,
        },
        exampleUserPrompt: '帮我判断',
        exampleReply: '结论先行。',
      },
    ],
    voice: {
      presetId: 'clarity',
      warmth: 2,
      directness: 4,
      challenge: 3,
      playfulness: 1,
      detail: 3,
      initiative: 3,
      structure: 4,
      restraint: 4,
      neutralMode: false,
    },
    renderedPrompt: '[Persona Voice]\nThis block controls expression style only.',
  })),
  updatePersonaVoiceProfile: vi.fn(async (voice) => ({
    presets: [],
    voice,
    renderedPrompt: '[Persona Voice]\nThis block controls expression style only.',
  })),
}))

describe('PersonaStudio', () => {
  it('loads presets and shows the style-only prompt preview', async () => {
    renderWithProviders(<PersonaStudio />)
    await waitFor(() => expect(screen.getByText('Persona Studio')).toBeInTheDocument())
    expect(screen.getByText('Clarity')).toBeInTheDocument()
    expect(screen.getByText('结论先行。')).toBeInTheDocument()
    expect(screen.getByText(/expression style only/)).toBeInTheDocument()
  })
})
