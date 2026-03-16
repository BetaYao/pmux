# pmux Hooks Setup

pmux can receive real-time agent status events from AI coding tools via a local HTTP webhook server (default port 7070). This replaces unreliable terminal text parsing with precise event-driven status detection.

## Automatic Setup

pmux detects installed tools at startup and offers one-click installation. Look for the setup banner in the sidebar when a supported tool is installed but not yet configured.

## Manual Setup

If you prefer to configure manually, use the examples below.

---

### Claude Code

Edit `~/.claude/settings.json` (create if it doesn't exist):

```json
{
  "hooks": {
    "SessionStart": [{"hooks": [{"type": "http", "url": "http://localhost:7070/webhook", "async": true}]}],
    "PreToolUse":   [{"hooks": [{"type": "http", "url": "http://localhost:7070/webhook", "async": true}]}],
    "Stop":         [{"hooks": [{"type": "http", "url": "http://localhost:7070/webhook", "async": true}]}],
    "Notification": [{"hooks": [{"type": "http", "url": "http://localhost:7070/webhook", "async": true}]}]
  }
}
```

**Status mapping:**
| Hook event | pmux status |
|-----------|------------|
| `SessionStart`, `PreToolUse` | Running |
| `Stop` | Idle |
| `Notification` | Waiting |

---

### Gemini CLI

Edit `~/.gemini/settings.json`:

```json
{
  "hooks": {
    "SessionStart": [{"hooks": [{"type": "command", "command": "curl -sf -X POST 'http://localhost:7070/webhook' -H 'Content-Type: application/json' -d @-"}]}],
    "BeforeTool":   [{"hooks": [{"type": "command", "command": "curl -sf -X POST 'http://localhost:7070/webhook' -H 'Content-Type: application/json' -d @-"}]}],
    "AfterAgent":   [{"hooks": [{"type": "command", "command": "curl -sf -X POST 'http://localhost:7070/webhook' -H 'Content-Type: application/json' -d @-"}]}],
    "Notification": [{"hooks": [{"type": "command", "command": "curl -sf -X POST 'http://localhost:7070/webhook' -H 'Content-Type: application/json' -d @-"}]}]
  }
}
```

**Status mapping:**
| Hook event | pmux status |
|-----------|------------|
| `SessionStart`, `BeforeTool` | Running |
| `AfterAgent` | Idle |
| `Notification` | Waiting |

---

### Codex (OpenAI)

Codex supports only `SessionStart` and `Stop` hook events (command type only, no HTTP).

Edit `~/.codex/config.toml` (create if needed):

```toml
[[hooks.SessionStart]]
command = "curl -sf -X POST 'http://localhost:7070/webhook' -H 'Content-Type: application/json' -d '{\"hook_event_name\":\"SessionStart\",\"cwd\":\"$PWD\"}'"

[[hooks.Stop]]
command = "curl -sf -X POST 'http://localhost:7070/webhook' -H 'Content-Type: application/json' -d '{\"hook_event_name\":\"Stop\",\"cwd\":\"$PWD\"}'"
```

---

### Aider

Edit `~/.aider.conf.yml` (create if needed):

```yaml
notifications: true
notifications-command: "curl -sf -X POST 'http://localhost:7070/webhook' -H 'Content-Type: application/json' -d '{\"hook_event_name\":\"aider_waiting\",\"cwd\":\"$(pwd)\"}' > /dev/null 2>&1"
```

> **Note:** Aider only sends one event (when waiting for input). Running status is still detected via terminal output for Aider.

---

### opencode

opencode uses a TypeScript plugin system. A pmux integration plugin is on the roadmap. For now, opencode status is detected via terminal output parsing.

---

## Custom Port

If you change the default port in pmux settings (`~/.config/pmux/config.json`):

```json
{
  "webhook": {
    "enabled": true,
    "port": 8080
  }
}
```

Update all hook URLs accordingly (`http://localhost:8080/webhook`).

## Testing

You can test the webhook manually:

```bash
curl -X POST http://localhost:7070/webhook \
  -H 'Content-Type: application/json' \
  -d '{"hook_event_name":"Stop","cwd":"/your/project","session_id":"test"}'
# Expected response: OK
```

## Troubleshooting

- **pmux not receiving events:** Check that pmux is running and the port matches your config
- **Port conflict:** Change `webhook.port` in `~/.config/pmux/config.json`
- **Hook not triggering:** Verify the tool's settings.json syntax with a JSON validator
