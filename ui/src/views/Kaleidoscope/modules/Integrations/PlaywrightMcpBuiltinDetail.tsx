import { Badge } from '@/components/ui/badge'
import { SettingsCard, SettingsRow, SettingsSection } from '@/features/settings'

export function PlaywrightMcpBuiltinDetail() {
  return (
    <div className="mt-4">
      <SettingsSection title="Playwright MCP" description="Built-in official MCP server">
        <SettingsCard>
          <SettingsRow
            label="Status"
            description="Advanced provider, configured through Browser Runtime Control Center"
          >
            <Badge variant="secondary">Built-in integration</Badge>
          </SettingsRow>
          <SettingsRow label="Raw MCP exposure" description="Raw MCP tools locked off" />
          <SettingsRow label="Action boundary" description="Wrapped browser actions only" />
          <SettingsRow label="Runtime source" description="Official npx @playwright/mcp@latest" />
          <SettingsRow label="Server startup" description="MCP Manager managed" />
          <SettingsRow
            label="Last server probe"
            description="Read from Browser Runtime Control Center probe history"
          />
          <SettingsRow
            label="Last action envelope"
            description="uClaw Browser Runtime adapter calls only"
          />
          <SettingsRow
            label="Last artifact/error route"
            description="Artifacts stay under Browser Runtime Supervisor ownership"
          />
        </SettingsCard>
      </SettingsSection>
    </div>
  )
}
