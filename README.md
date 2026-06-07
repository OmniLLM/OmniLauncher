# OmniLauncher

OmniLauncher is a keyboard-first launcher with local search, AI mode, plugins, and slash commands.

- **Alt+Space** opens the launcher
- Type normal text for app search and built-in actions
- Type **`?`** or **`ai `** for AI mode
- Type **`/`** for structured slash commands
- Press **Ctrl+,** for Settings

---

## Quick Start

### 1) Install

Install the project dependencies:

```bash
npm install
cd src-tauri && cargo fetch
```

For day-to-day development you can run:

```bash
make start
```

### 2) Configure the backend token first

If you are using a separate backend, set the backend token on the backend machine **before** configuring the UI.

Backend example:

```bash
export OMNILAUNCHER_AUTH_TOKEN="your-shared-backend-token"
make start-backend
```

Notes:

- If `OMNILAUNCHER_AUTH_TOKEN` is set, that value is used by the backend
- If it is not set, the backend generates a random token on startup
- That random token is fine for same-machine use, but not for split-machine setups

### 3) Configure the UI

Open OmniLauncher, then press **Ctrl+,**:

1. Open the **General** tab
2. Set **Backend URL** to your backend address
3. Set **Backend Token** to the same token you set on the backend
4. Click **Save Settings**
5. Open the **AI** tab
6. Set your LLM provider values:
   - **Provider URL**
   - **API Key**
   - **Model**

Important:

- **Backend Token** authenticates OmniLauncher itself
- **API Key** authenticates your LLM provider
- Configure **Backend Token first**, then configure the LLM token in the UI

---

## Everyday Use

### Launcher mode

- Type an app name, file path, command, or query
- Use the arrow keys to select a result
- Press **Enter** to run it
- Press **Esc** to close or clear

### AI mode

- Start a query with **`?`** or **`ai `**
- Ask for explanations, summaries, code help, or tool-assisted tasks
- AI responses can call built-in tools automatically

### Slash commands

Type **`/`** to use structured commands such as:

- `/app` — launch an app
- `/run` — run a shell command
- `/open` — open a file, app, or URL
- `/find` / `/grep` / `/ls` — search and inspect files
- `/todo` — manage todos
- `/web` — search the web

### Other operations

- **Plugins** — install, update, and remove external plugins
- **Skills** — manage AI skills
- **Sessions** — switch or clear AI conversations
- **Dashboard** — inspect backend data in a browser

---

## Common commands

```bash
make start       # start the app
make stop        # stop the app
make test        # run the full test suite
make status      # show backend status
make logs        # show logs
```

---

## Config files

Main settings:

```text
~/.config/omnilauncher/settings.json
```

Backend token fallback file:

```text
~/.config/omnilauncher/server-token
```

---

## Troubleshooting

- If settings do not save, confirm the backend token matches on both sides
- If AI requests fail, check the **AI** tab values
- If the UI can reach the backend but saving still fails, verify the backend was started with the same `OMNILAUNCHER_AUTH_TOKEN`
