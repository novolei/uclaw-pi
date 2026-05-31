import { describe, it, expect } from 'vitest'

import * as agent from './index'

// Barrel smoke test. Asserts the stable public surface (the re-exported agent
// IPC bridge); per-component coverage lives in each component's own render
// test. Component re-exports grow per migration commit, so this stays focused
// on the bridge to keep every intermediate commit green.
describe('features/agent barrel', () => {
  it('re-exports the agent bridge commands', () => {
    expect(typeof agent.sendAgentMessage).toBe('function')
    expect(typeof agent.stopAgent).toBe('function')
    expect(typeof agent.updateAgentSessionTitle).toBe('function')
    expect(typeof agent.moveAgentSessionToWorkspace).toBe('function')
    expect(typeof agent.getSafetyPolicy).toBe('function')
    expect(typeof agent.setSafetyMode).toBe('function')
  })

  it('re-exports the session-eval event subscriptions', () => {
    expect(typeof agent.onSessionEvalComplete).toBe('function')
    expect(typeof agent.onSessionEvalWarning).toBe('function')
  })
})
