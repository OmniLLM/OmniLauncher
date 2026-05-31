# OmniLauncher External Plugins

External plugins let you extend OmniLauncher with custom search providers and
actions written in **any language** (Bash, Python, Node.js, Ruby, …) without
modifying the core codebase.

---

## Installing a plugin

Type `plugins` (or `pm`) in the launcher and press Enter to open the **Plugin
Manager** panel.  Paste a Git URL or a local directory path and click
**Install**.

You can also manage plugins programmatically:

| Action | What to type |
|--------|-------------|
| Open Plugin Manager | `plugins` or `pm` |
| Install from Git | paste URL in Plugin Manager |
| Install from local dir | paste path in Plugin Manager |
| Remove | click Remove in Plugin Manager |

---

## Where plugins live

```
~/.omnilauncher/plugins/
  my-plugin/
    plugin.json    ← required manifest
    run.sh         ← executable entry point (any language)
```

---

## `plugin.json` manifest

```json
{
  "name": "my-plugin",
  "description": "Does something useful",
  "version": "1.0.0",
  "keyword": "myplugin",
  "icon": "🔌",
  "entry": "run.sh",
  "query_timeout_ms": 3000
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | ✅ | Unique plugin identifier |
| `description` | ✅ | Short one-line description |
| `version` | ✅ | Semver string |
| `keyword` | ✗ | If set, the plugin only runs when the query starts with this prefix |
| `icon` | ✗ | Emoji shown for results that don't supply their own icon |
| `entry` | ✅ | Relative path to the executable within the plugin directory |
| `query_timeout_ms` | ✗ | Per-plugin query timeout in milliseconds. Default `3000`, capped at `5000`. |

---

## Communication protocol (stdin/stdout JSON)

OmniLauncher spawns the entry executable for every query/execute call.  Each
invocation is **stateless** — the process starts, handles one request, then
exits.

### Query request (launcher search)

Sent on **stdin**:
```json
{"op": "query", "query": "user typed text"}
```

Expected **stdout** response:
```json
{
  "results": [
    {
      "id": "unique-item-id",
      "title": "Result Title",
      "subtitle": "Optional detail line",
      "icon": "🔌",
      "score": 80,
      "action_type": "shell",
      "action_data": "echo hello world"
    }
  ]
}
```

Return an empty array when there are no matches:
```json
{"results": []}
```

**Timeout:** 3 seconds by default (configurable per-plugin via the
`query_timeout_ms` manifest field, capped at 5 seconds).  If your plugin
doesn't respond in time OmniLauncher returns no results for that query (it does
**not** crash).

### Execute request (user picks a result)

Sent on **stdin**:
```json
{"op": "execute", "id": "unique-item-id", "action_data": "echo hello world"}
```

Expected **stdout** response:
```json
{"output": "hello world"}
```

**Timeout:** 10 seconds.

### `action_type` values

| Value | Behaviour |
|-------|-----------|
| `shell` | Run `action_data` as a shell command (Windows: `cmd /C start "" <data>`) |
| `url` | Open `action_data` as a URL in the default browser |
| `open` | Open `action_data` with `xdg-open` / `open` / Explorer |
| `copy` | Copy `action_data` to the clipboard |
| `plugin_execute` | Re-invoke this plugin with `op=execute`; the plugin performs the action itself and returns `{"output": "..."}`. Use this when your action can't be expressed as a single shell/URL string (e.g. calling a Win32 API). |

---

## Example plugins

### Python — dictionary lookup

`plugin.json`:
```json
{
  "name": "dict",
  "description": "Look up word definitions via Free Dictionary API",
  "version": "1.0.0",
  "keyword": "dict",
  "icon": "📖",
  "entry": "run.py"
}
```

`run.py`:
```python
#!/usr/bin/env python3
import json, sys, urllib.request

request = json.loads(sys.stdin.readline())
op = request.get("op")
query = request.get("query", "").removeprefix("dict").strip()

if op == "query":
    if not query:
        print(json.dumps({"results": []}))
        sys.exit(0)
    try:
        url = f"https://api.dictionaryapi.dev/api/v2/entries/en/{query}"
        with urllib.request.urlopen(url, timeout=2) as r:
            data = json.load(r)
        meaning = data[0]["meanings"][0]["definitions"][0]["definition"]
        result = {
            "id": f"dict-{query}",
            "title": query,
            "subtitle": meaning[:120],
            "icon": "📖",
            "score": 90,
            "action_type": "copy",
            "action_data": meaning,
        }
        print(json.dumps({"results": [result]}))
    except Exception:
        print(json.dumps({"results": []}))

elif op == "execute":
    action_data = request.get("action_data", "")
    print(json.dumps({"output": action_data}))
```

Make it executable:
```bash
chmod +x run.py
```

---

### Node.js — GitHub repo search

`plugin.json`:
```json
{
  "name": "gh-search",
  "description": "Search GitHub repositories",
  "version": "1.0.0",
  "keyword": "gh",
  "icon": "🐙",
  "entry": "run.js"
}
```

`run.js`:
```js
#!/usr/bin/env node
const lines = [];
process.stdin.on("data", (d) => lines.push(d));
process.stdin.on("end", async () => {
  const req = JSON.parse(lines.join(""));
  const query = (req.query ?? "").replace(/^gh\s*/, "").trim();

  if (req.op === "execute") {
    process.stdout.write(JSON.stringify({ output: req.action_data }) + "\n");
    return;
  }

  if (!query) {
    process.stdout.write(JSON.stringify({ results: [] }) + "\n");
    return;
  }

  try {
    const res = await fetch(
      `https://api.github.com/search/repositories?q=${encodeURIComponent(query)}&per_page=5`
    );
    const data = await res.json();
    const results = (data.items ?? []).map((item, i) => ({
      id: `gh-${item.id}`,
      title: item.full_name,
      subtitle: item.description ?? "",
      icon: "🐙",
      score: 90 - i * 5,
      action_type: "url",
      action_data: item.html_url,
    }));
    process.stdout.write(JSON.stringify({ results }) + "\n");
  } catch {
    process.stdout.write(JSON.stringify({ results: [] }) + "\n");
  }
});
```

Make it executable:
```bash
chmod +x run.js
```

---

## Tips

- **Keyword prefix** — set `keyword` to limit when your plugin fires.  Queries
  that don't start with the keyword are never sent to your plugin, saving CPU.
- **Stateless** — each call is a fresh process.  Store state in files or a
  database if you need persistence.
- **Error safety** — OmniLauncher catches timeouts and missing executables
  gracefully; a broken plugin never crashes the launcher.
- **Debugging** — write debug output to **stderr** (OmniLauncher discards it).
  Only JSON goes to stdout.
- **Windows** — on Windows, make sure your entry file has a registered
  association (`.py` → Python, `.js` → Node) or use a `.cmd` / `.bat` wrapper.
