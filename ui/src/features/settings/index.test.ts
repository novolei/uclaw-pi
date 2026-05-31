import { describe, it, expect } from 'vitest'

import * as settings from './index'

describe('features/settings barrel', () => {
  it('exposes the settingsBridge re-export', () => {
    expect(settings.settingsBridge).toBeDefined()
    expect(typeof settings.settingsBridge.getHttpApiEnabled).toBe('function')
    expect(typeof settings.settingsBridge.setHttpApiEnabled).toBe('function')
  })
})
