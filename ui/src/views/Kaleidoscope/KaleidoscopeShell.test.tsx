import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { createStore } from 'jotai'
import { KaleidoscopeShell } from './KaleidoscopeShell'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'
import { automationsSubviewAtom, marketplaceSelectedSlugAtom } from '@/atoms/marketplace'

vi.mock('@/lib/tauri-bridge', () => ({
  getUserProfile: vi.fn().mockResolvedValue({ userName: 'User', avatar: null }),
  listAutomationsHumane: vi.fn().mockResolvedValue([]),
  listLearnedSkills: vi.fn().mockResolvedValue([]),
  toggleLearnedSkill: vi.fn().mockResolvedValue(undefined),
  deleteLearnedSkill: vi.fn().mockResolvedValue(undefined),
  proposeSkillConsolidation: vi.fn().mockResolvedValue({ clusters: [] }),
  backfillSkillKeywords: vi.fn().mockResolvedValue({ backfilledSkills: 0, totalLearnedSkills: 0, keywordsInserted: 0 }),
  listSkills: vi.fn().mockResolvedValue([]),
  toggleSkill: vi.fn().mockResolvedValue(true),
  forkSkillToUser: vi.fn().mockResolvedValue(''),
  reloadSkills: vi.fn().mockResolvedValue([]),
  listMcpServers: vi.fn().mockResolvedValue([]),
  listMcpTools: vi.fn().mockResolvedValue([]),
  addMcpServer: vi.fn().mockResolvedValue({}),
  updateMcpServer: vi.fn().mockResolvedValue({}),
  removeMcpServer: vi.fn().mockResolvedValue(true),
  toggleMcpServer: vi.fn().mockResolvedValue(true),
  restartMcpServer: vi.fn().mockResolvedValue(true),
  connectMcpServer: vi.fn().mockResolvedValue(true),
}))

// automation 组件子树重 —— stub 掉，本测试只关心 KaleidoscopeShell 的模块路由。
vi.mock('@/components/automation/AutomationHub', () => ({
  AutomationHub: () => <div data-testid="automation-hub" />,
}))
vi.mock('@/components/automation/StoreView', () => ({
  StoreView: () => <div data-testid="store-view" />,
}))
vi.mock('@/components/automation/StoreDetail', () => ({
  StoreDetail: () => <div data-testid="store-detail" />,
}))
vi.mock('@/components/automation/AppsTab', () => ({
  AppsTab: () => <div data-testid="apps-tab" />,
}))
vi.mock('@/components/memory/MemoryGraphView', () => ({
  MemoryGraphView: () => <div data-testid="memory-graph-view" />,
}))
vi.mock('@/features/settings', () => ({
  SkillConsolidationDialog: () => null,
  SkillEvolutionTab: () => <div data-testid="skill-evolution-tab" />,
}))

describe('KaleidoscopeShell', () => {
  it('renders the rail and the humans module (AutomationHub) by default', () => {
    renderWithProviders(<KaleidoscopeShell />)
    expect(screen.getByRole('button', { name: /数字人/ })).toBeInTheDocument()
    expect(screen.getByTestId('automation-hub')).toBeInTheDocument()
  })

  it('renders StoreView for the store module', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'store')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByTestId('store-view')).toBeInTheDocument()
    expect(screen.queryByTestId('automation-hub')).not.toBeInTheDocument()
  })

  it('renders StoreDetail when subview is store-detail AND a slug is selected', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'store')
    store.set(automationsSubviewAtom, 'store-detail')
    store.set(marketplaceSelectedSlugAtom, 'some-slug')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByTestId('store-detail')).toBeInTheDocument()
    expect(screen.queryByTestId('store-view')).not.toBeInTheDocument()
  })

  it('falls back to StoreView when subview is store-detail but no slug is selected', () => {
    // automationsSubviewAtom is persisted (atomWithStorage); a restart can
    // restore 'store-detail' while the non-persisted slug atom is null. That
    // orphan state must not strand StoreDetail on an unbreakable spinner.
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'store')
    store.set(automationsSubviewAtom, 'store-detail')
    // marketplaceSelectedSlugAtom left at its default (null)
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByTestId('store-view')).toBeInTheDocument()
    expect(screen.queryByTestId('store-detail')).not.toBeInTheDocument()
  })

  it('renders AppsTab for the apps module', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'apps')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByTestId('apps-tab')).toBeInTheDocument()
  })

  it('renders MemoryGraphView for the memory module', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'memory')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByTestId('memory-graph-view')).toBeInTheDocument()
  })

  it('renders SkillsModule for the skills module', async () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'skills')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(await screen.findByRole('heading', { name: '技能' })).toBeInTheDocument()
  })

  it('renders IntegrationsModule for the integrations module', async () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'integrations')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(await screen.findByText('集成 · MCP')).toBeInTheDocument()
  })

  it('renders the ComingSoon placeholder for a not-yet-built module', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'artifacts')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByText('即将到来 · Phase 2')).toBeInTheDocument()
    expect(screen.queryByTestId('automation-hub')).not.toBeInTheDocument()
  })
})
