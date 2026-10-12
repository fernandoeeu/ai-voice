![Claude + Echo: your AI coding agent, now with a voice](https://raw.githubusercontent.com/henriquearthur/ai-voice/main/assets/claude-echo.png)

# Jarvis

An [MCP](https://modelcontextprotocol.io) server that lets AI agents talk to you out loud through your Amazon Echo, in the Alexa voice, plus a small dashboard around it.

Ask Claude Code or Codex to "tell me when the build is done", walk away from the screen, and hear the answer from the Echo in the room. It works with any client that speaks MCP.

```text
you    > run the migration and tell me out loud how it went
agent  > (calls speak) "Migration finished. All 42 tables were updated and the tests pass."
```

> [!NOTE]
> Jarvis is a fork of [AI Voice](https://github.com/henriquearthur/ai-voice) by Henrique Arthur (MIT). The original project is the one to credit for the idea and the first implementation. This fork restructures it as a Bun + Turborepo monorepo and adds a dashboard.

Voice tools, available on both transports:

| Tool | What it does |
| --- | --- |
| `list_devices` | Lists the physical Echo devices on your Amazon account. Plays no audio. |
| `speak` | Speaks up to 4000 characters of plain text on one Echo. Uses your default Echo when `device_serial` is omitted. |

Mac tools, available **only over stdio** (the HTTP endpoint has no authentication, so it stays voice-only): `reminders_lists`, `reminders_list`, `reminders_add`, `calendar_today`, `calendar_events`, `calendar_calendars`, `maps_search`, `maps_directions` and `phone_call`. They drive Reminders, Calendar and Maps through `osascript`, and `phone_call` hands a number to macOS, which asks you to confirm before dialing. See the [configuration reference](docs/configuration.md#tools).

> [!IMPORTANT]
> Jarvis is an unofficial, community project. It is not affiliated with or endorsed by Amazon, and it relies on the private APIs the Alexa app uses, through [`alexa-remote2`](https://github.com/Apollon77/alexa-remote). Amazon can change those APIs at any time. Read [How the Alexa integration works](docs/alexa-integration.md) before trusting it with your account.

## Install on a Mac (one command)

On macOS, one script installs and wires everything for the current user. It is idempotent: run it again after a `git pull` to update. Until the Jarvis pull request is merged, check it out after cloning with `gh pr checkout <number>`.

```sh
git clone https://github.com/henriquearthur/ai-voice.git jarvis
cd jarvis
./scripts/setup.sh --dry-run   # shows what it would do, changes nothing
./scripts/setup.sh
```

It checks the prerequisites and says what is missing, runs `bun install`, then installs:

| Piece | Where | What it does |
| --- | --- | --- |
| Local voice server (Kokoro) | `~/.local/share/jarvis/tts`, LaunchAgent `dev.jarvis.tts` on `127.0.0.1:7349` | Natural pt-BR speech on the Mac with [Kokoro-82M](https://huggingface.co/mlx-community/Kokoro-82M-bf16) through `mlx-audio`. Creates a Python 3.12 venv from `services/tts/requirements.lock.txt` and downloads the model with only the pt-BR voices (about 330 MB). Apple Silicon only; without it the Mac speaks with `say`. |
| Speech commands | `~/.local/bin/jarvis-fala`, `jarvis-fala-resposta`, `jarvis-avisa`, `jarvis-mcp-stdio` | `jarvis-fala "text"` speaks on the Mac, one speech at a time. The other two are optional Claude Code hooks. `jarvis-mcp-stdio` starts the MCP server and reads the Grok Bot secrets from the Keychain. |
| Dashboard | LaunchAgent `dev.jarvis.dashboard` on `http://127.0.0.1:7350` | Production build of `apps/dashboard`, kept running. |
| Voice listening (Escuta) | `~/.local/share/jarvis/bin/Escuta.app`, LaunchAgent `dev.jarvis.escuta` | Menu bar app that listens for "Jarvis", transcribes on the device and hands the request to an agent session running in herdr. Needs macOS 26 and herdr; skipped when herdr is missing. |
| MCP server in Claude Code | `claude mcp add ai-voice -s user` | Registered only if the `claude` CLI exists and `ai-voice` is not registered yet. |

Everything optional is off by default:

| Flag | Adds |
| --- | --- |
| `--com-echo` | The Amazon Echo: asks for your marketplace and default Echo, then tells you how to sign in. |
| `--com-grok` | The Grok Bot callback server (`dev.jarvis.callback` on `127.0.0.1:7348`). Secrets go to the Keychain and public exposure goes through Tailscale Funnel, both by hand. |
| `--sem-voz`, `--sem-dashboard`, `--sem-escuta`, `--sem-mcp` | Skip that piece. `--com-escuta` installs Escuta even without herdr. |
| `--sim` | Asks nothing; uses the config file, the environment and the defaults. |

Your answers are kept in `~/.config/jarvis/jarvis.env` (no secrets there), which the dashboard and Escuta installers also read. At the end the script prints what is left to do by hand: macOS permissions (Calendars, Reminders, Automation, and Microphone and Speech Recognition for Escuta), the Amazon sign-in if you want the Echo, and the Keychain and Funnel steps if you want the Grok Bot. See [Mac setup](docs/setup.md) for every file it writes, the hooks and how to uninstall.

The sections below set up the MCP server by hand, for other clients or other operating systems.

## Layout

This is a Bun workspace managed by Turborepo.

| Path | Package | What it is |
| --- | --- | --- |
| `packages/alexa` | `@jarvis/alexa` | Alexa session, devices, speech and login, built on Effect v4. |
| `packages/apple` | `@jarvis/apple` | Reminders, Calendar, Maps and Phone on the Mac, through `osascript` and `open`. Effect v4. |
| `packages/grokbot` | `@jarvis/grokbot` | In progress. Send-only and at-most-once: it sends a message and has no result channel. |
| `packages/voice` | `@jarvis/voice` | Speech on the Mac: the local TTS server first, `say` as fallback, serialized with a lock shared with `jarvis-fala`. |
| `packages/announce` | `@jarvis/announce` | Turns agent events into short, safe pt-BR announcements. |
| `apps/mcp` | `@jarvis/mcp` | The MCP server (bin `jarvis-voice`: stdio and Streamable HTTP on `/mcp`) and the `login` and `devices` commands. Built on Effect v4. |
| `apps/dashboard` | `@jarvis/dashboard` | SvelteKit dashboard: `/` (Alexa), `/lembretes`, `/agenda` and `/ir`. |
| `apps/escuta` | Swift package | Always-on voice listening in the menu bar (macOS 26+). |
| `services/tts` | Python | The Kokoro voice server, its lock file and installer, and the `fala-kokoro.sh` client. |
| `services/bin`, `services/launchd` | Shell | The `~/.local/bin` commands and the LaunchAgent templates that `scripts/setup.sh` installs. |

Environment variables keep the `AI_VOICE_*` names from the original project.

## Requirements

- [Bun](https://bun.sh) 1.3 or newer. The exact version is pinned in `packageManager` in `package.json`.
- macOS. The Mac tools and the dashboard's Reminders, Agenda and Ir pages need it, and macOS asks for Automation permission the first time.
- For `scripts/setup.sh`: Xcode Command Line Tools (`xcode-select --install`), and [uv](https://docs.astral.sh/uv/) or Python 3.12 for the voice server on Apple Silicon. Escuta needs macOS 26 and herdr.
- For the Echo only: an Amazon account with at least one Echo device.
- The machine running Jarvis must be on and online while agents use it.

## Quick start

Clone the repository and install the dependencies once:

```sh
bun install
```

### 1. Sign in to Amazon

```sh
bun apps/mcp/src/main.ts login
```

When the terminal says the proxy is ready, open **http://127.0.0.1:3456/** in a browser on the same machine and sign in to Amazon, including MFA or CAPTCHA if asked. You type your password in the browser, never into an agent or chat. The proxy only listens on loopback and shuts down when login completes, after ten minutes, or on Ctrl+C. Logging in may register a virtual device on your Amazon account.

If your account is not on `amazon.com`, pass your marketplace for the login:

```sh
AI_VOICE_AMAZON_PAGE=amazon.co.uk bun apps/mcp/src/main.ts login
```

The session is stored in `~/.local/share/ai-voice/` with owner-only permissions. It holds tokens for your Amazon account: do not share or commit that folder.

### 2. Pick your Echo

```sh
bun apps/mcp/src/main.ts devices
```

This lists your Echo devices without playing audio. Copy the `serial` of the one that should speak by default.

### 3. Connect your agent

Speech stays disabled until you set `AI_VOICE_ENABLE_SPEECH=1`, so nothing can make noise in your home before you opt in. In the examples below, replace `SERIAL` with the serial from step 2 and `/ABS/PATH` with the absolute path of this repository. Set `AI_VOICE_LOCALE` if your Echo does not speak `en-US`, for example `pt-BR` or `de-DE`.

If your client does not inherit your shell `PATH` (common with nvm or fnm), replace `bun` with its absolute path, from `which bun`.

#### Claude Code

```sh
claude mcp add ai-voice --scope user \
  --env AI_VOICE_ENABLE_SPEECH=1 --env AI_VOICE_DEFAULT_DEVICE=SERIAL \
  -- bun /ABS/PATH/apps/mcp/src/main.ts
```

#### Codex

```sh
codex mcp add ai-voice \
  --env AI_VOICE_ENABLE_SPEECH=1 --env AI_VOICE_DEFAULT_DEVICE=SERIAL \
  -- bun /ABS/PATH/apps/mcp/src/main.ts
```

Or add it to `~/.codex/config.toml`:

```toml
[mcp_servers.ai-voice]
command = "bun"
args = ["/ABS/PATH/apps/mcp/src/main.ts"]
env = { AI_VOICE_ENABLE_SPEECH = "1", AI_VOICE_DEFAULT_DEVICE = "SERIAL" }
```

#### Any other MCP client

Jarvis is a standard stdio MCP server. Most clients (Claude Desktop, Cursor, VS Code, Gemini CLI and others) accept this JSON shape in their MCP configuration file:

```json
{
  "mcpServers": {
    "ai-voice": {
      "command": "bun",
      "args": ["/ABS/PATH/apps/mcp/src/main.ts"],
      "env": { "AI_VOICE_ENABLE_SPEECH": "1", "AI_VOICE_DEFAULT_DEVICE": "SERIAL" }
    }
  }
}
```

Clients that speak Streamable HTTP can also connect to a Jarvis instance running on another machine. See [Serve Jarvis over the network](docs/network.md).

### 4. Try it

Restart or reconnect your client, then ask:

> Say "hello from my agent" out loud.

To make an agent speak without being asked each time, add a rule to its instruction file (`CLAUDE.md`, `AGENTS.md` or equivalent), for example: *"When you finish a task that took more than a few minutes, call `speak` with a one-sentence summary."*

### Dashboard

```sh
bunx turbo run dev --filter=@jarvis/dashboard
```

Pages: `/` sends speech to an Echo, `/lembretes` lists and creates reminders, `/agenda` shows today or the week, and `/ir` opens Maps searches and routes or starts a call after an explicit confirmation. The dashboard reads the same `AI_VOICE_*` variables and has no authentication of its own, so keep it on a machine and network you trust. See [the dashboard section](docs/configuration.md#dashboard).

## What to expect

- `speak` returns `status: "accepted"` with `playback_confirmed: false`. The Alexa API accepted the request; only you can confirm you heard it.
- Jarvis never retries speech. After a timeout or error the Echo may already have received the request, and retrying would repeat it.
- Text longer than 250 characters is split at sentence or word boundaries and sent as one ordered sequence.
- One server speaks one request at a time, on one physical Echo. Device groups, volume control and SSML are out of scope.
- The session renews itself. If renewal fails, the tool asks you to run `bun apps/mcp/src/main.ts login` again.

## Documentation

- [Configuration reference](docs/configuration.md): every environment variable, the tool contracts, and troubleshooting.
- [Serve Jarvis over the network](docs/network.md): run it once on a home server and use it from every machine.
- [How the Alexa integration works](docs/alexa-integration.md): the private APIs involved, what was validated, security and known limits.
- [Mac setup](docs/setup.md): what `scripts/setup.sh` installs, the files it writes, the optional parts and how to uninstall.

## Development

```sh
bun install
bun run check   # type checks across the workspace
bun run test
bun run build
```

These run through Turborepo, so each package is checked, tested or built in dependency order and results are cached. Run the dashboard with the `dev` command shown in [Dashboard](#dashboard).

Tests use simulated Amazon devices and transport, need no Amazon account and play no audio.

Issues and pull requests are welcome. Reports from marketplaces, locales and Echo models other than the ones listed in [what was validated](docs/alexa-integration.md#what-was-validated) are especially useful.

## License

[MIT](LICENSE). Copyright (c) 2026 Henrique Arthur, the original author of AI Voice, and Fernando Antonio for the changes in this fork. Amazon, Alexa and Echo are trademarks of Amazon.com, Inc. or its affiliates.
