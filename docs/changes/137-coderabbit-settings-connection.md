# 137 — CodeRabbit account connection in Settings (2026-10-17)

## Scope

The Settings window's Accounts & Integrations panel and the Command Hub's Connections panel share `McpServersPanel`. Both now show a CodeRabbit card under **MCP & Integrations**. The card is marked **CLI** because CodeRabbit is not exposed as an inbound MCP server: its documented MCP support makes CodeRabbit the client of other MCP servers.

The card uses CodeRabbit's unmodified orange mark from its official brand page. Source: <https://www.coderabbit.ai/content/assets/orange-logomark.svg>; usage guidance: <https://www.coderabbit.ai/brand>.

## Implemented

- `src-tauri/src/coderabbit.rs` locates the official per-user Windows CLI, reads `coderabbit auth status --agent`, starts `coderabbit auth login --agent --region us|eu`, and runs `coderabbit auth logout --agent`.
- The official CLI owns browser OAuth and local credentials. NEXUS does not collect a CodeRabbit password, pasted OAuth token, or API key for this flow.
- The backend accepts only an explicit positive CLI authentication status before reporting connected. Unknown or failed status output stays an error, never a false success.
- Sign-in times out after five minutes; status and sign-out have 20-second limits. A second sign-in/sign-out cannot start while one account action is active. The CLI child is killed if its command future is dropped.
- `McpServersPanel.tsx` displays installed, connected, disconnected, and error states; offers US/EU choice, Connect, Disconnect, Check connection, official installation documentation, and the CodeRabbit dashboard. Other MCP cards keep their existing vault and delegation interface.

## Scope boundary

This change implements the **account connection and settings interface**. It does not yet route voice review commands through the main command center, run `coderabbit review --agent`, fetch PR comments, or post CodeRabbit review commands. The card explains that PR reviews also require repository access and a CodeRabbit installation on the repo. Do not describe this card as proof that a particular repository is ready for PR review.

## Verification

- `npx tsc --noEmit` — passed.
- `npx vitest run src/settings-sidebar/mcpRegistry.test.ts` — 4 passed.
- `npm run build` — passed; the official mark was copied to `dist/brands/`.
- `cargo check --features custom-protocol,admin-brain` — passed.
- `cargo test --lib coderabbit::tests --features custom-protocol,admin-brain` — 1 passed.
- Live OAuth was not exercised: the official CodeRabbit CLI is not installed on this machine.

## Primary references

- Browser OAuth and agent-readable status: <https://docs.coderabbit.ai/cli/reference>
- Windows CLI installation and signed binary location: <https://docs.coderabbit.ai/cli/windows>
- Agentic API key alternative and seat requirement: <https://docs.coderabbit.ai/cli/headless-cli-integration>
- CodeRabbit's MCP direction: <https://docs.coderabbit.ai/knowledge-base/mcp-context>
