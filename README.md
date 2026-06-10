<p align="center">
  <h1 align="center">OmniLauncher</h1>
</p>
<p align="center">A keyboard-first launcher with local search, AI mode, plugins, and slash commands.</p>
<p align="center">
  <a href="https://github.com/OmniLLM/OmniLauncher/actions/workflows/ci.yml"><img alt="Build status" src="https://img.shields.io/github/actions/workflow/status/OmniLLM/OmniLauncher/ci.yml?style=flat-square&branch=main" /></a>
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/github/license/OmniLLM/OmniLauncher?style=flat-square" /></a>
  <a href="https://github.com/OmniLLM/OmniLauncher/releases"><img alt="Release" src="https://img.shields.io/github/v/release/OmniLLM/OmniLauncher?style=flat-square&include_prereleases" /></a>
</p>

![OmniLauncher Chat UI](screenshot-chat-ui.png)

---

### Installation

OmniLauncher is built from source today (Tauri + Rust + a small Node frontend). Clone the repo, install deps, and start the app:

```bash
git clone https://github.com/OmniLLM/OmniLauncher.git
cd OmniLauncher

# Install JS deps and prefetch Rust crates
npm install
cd src-tauri && cargo fetch && cd ..

# Day-to-day dev: start frontend + backend together
make start
```

> [!TIP]
> Press **Ctrl+Shift+O** to open the launcher, **Ctrl+,** to open Settings, and **Esc** to close.

### Backend Token

If your backend runs on a separate machine, set its token **before** configuring the UI.

```bash
# On the backend machine
export OMNILAUNCHER_AUTH_TOKEN=<paste-your-token-here>
make start-backend
```

| Behavior                           | Setting                                         |
| ---------------------------------- | ----------------------------------------------- |
| Use a shared token (split-machine) | export `OMNILAUNCHER_AUTH_TOKEN` on the backend |
| Single-machine dev                 | leave it unset — a random one is generated      |
| Backend token file fallback        | `~/.config/omnilauncher/server-token`           |
| Frontend saved connection token    | `~/.config/omnilauncher/backend-token`          |

When the frontend starts and no saved connection token exists, it prompts for the backend token and stores it in `~/.config/omnilauncher/backend-token`. The token is not stored in `settings.json` and does not appear on the Settings page.

In the UI press **Ctrl+,**, open **General**, set **Backend URL**, then open **AI** and set **Provider URL**, **API Key**, and **Model**. Click **Save Settings**.

> [!IMPORTANT]
> The backend token authenticates OmniLauncher itself. **API Key** authenticates your LLM provider. They are separate — configure the backend token prompt first when using a split-machine backend.

### Modes

OmniLauncher switches mode based on the prefix you type.

- **Launcher** — bare text. Apps, files, URLs, built-in actions.
- **AI** — type `?` or `ai `. Explanations, summaries, code help, tool-assisted tasks.
- **Slash** — type `/`. Structured commands.

Common slash commands:

| Command                 | Purpose                         |
| ----------------------- | ------------------------------- |
| `/app`                  | launch an app                   |
| `/run`                  | run a shell command             |
| `/open`                 | open a file, app, or URL        |
| `/find`, `/grep`, `/ls` | search and inspect files        |
| `/todo`                 | manage todos                    |
| `/web`                  | search the web                  |
| `/skills`               | manage AI skills                |
| `/plugins`              | install, update, remove plugins |

### AGENTS.md Context

Drop an `AGENTS.md` file at `~/.config/omnilauncher/AGENTS.md` to use it as the primary global AI system prompt source. OmniLauncher also picks up legacy/project context files automatically (load order — most general first, most specific last):

1. `~/.config/omnilauncher/AGENTS.md` — primary global app system prompt
   - Legacy fallback: `~/.config/omnilauncher/AGENT.md`
2. `AGENT.md` walking upward from the current working directory — project context
3. `~/AGENT.md` — user-global

Missing files are silently skipped. See [`src-tauri/src/ai/agent_context.rs`](src-tauri/src/ai/agent_context.rs) for the loader.

### Common Commands

```bash
make start         # start frontend + backend
make stop          # stop everything
make restart       # restart everything
make status        # show backend status
make logs          # tail logs
make test          # run the full test suite
```

Run `make help` for the full target list.

### Config Files

| Path                                   | Purpose                            |
| -------------------------------------- | ---------------------------------- |
| `~/.config/omnilauncher/settings.json` | Main settings (UI-editable)        |
| `~/.config/omnilauncher/server-token`  | Backend token fallback             |
| `~/.config/omnilauncher/AGENTS.md`  | Primary global AI system prompt     |
| `~/.omnilauncher/`                     | Runtime data (DB, plugins, skills) |

### Troubleshooting

- **Settings won't save** — backend token mismatch between UI and backend. Re-set both.
- **AI requests fail** — check **AI** tab values (Provider URL, API Key, Model).
- **UI reaches backend but saves still fail** — backend wasn't started with the same `OMNILAUNCHER_AUTH_TOKEN`.
- **`make start` fails** — try `make stop` first, then `make start` again.

### Documentation

More detail in [`docs/`](./docs) and in [`SOURCE_FILES_MANIFEST.md`](./SOURCE_FILES_MANIFEST.md).

### Contributing

PRs welcome. Please run `make test` (or at least `cargo test --lib` + `npm test`) before submitting, and keep new behavior covered by tests.

### Security

Report security issues per [`SECURITY.md`](./SECURITY.md). Don't open public issues for vulnerabilities.

### Building on OmniLauncher

If you're shipping a project that uses "omnilauncher" in its name (e.g. `omnilauncher-plugin-foo`), please add a note to your README clarifying that it isn't built by the OmniLauncher team and isn't affiliated with us.

---

**Project** [OmniLLM/OmniLauncher](https://github.com/OmniLLM/OmniLauncher) · **License** see [LICENSE](./LICENSE)
