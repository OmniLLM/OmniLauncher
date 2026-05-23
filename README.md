# OmniLauncher

A fast, extensible application launcher for Windows — inspired by Flow.Launcher.
Summon it anywhere with **Alt+Space**.

## Features

- 🚀 **App Launcher** — searches Start Menu shortcuts instantly
- 🔍 **Web Search** — `g ` Google · `yt ` YouTube · `gh ` GitHub
- 🧮 **Calculator** — prefix `=`, evaluates expression, copies result to clipboard
- 📁 **File Search** — prefix `f ` or `open ` to find files and folders
- 📋 **Clipboard History** — prefix `cb ` to search and paste previous clips
- 🤖 **OmniLLM AI** — prefix `ai ` to ask questions via your local OmniLLM proxy
- ⚙️ **Settings UI** — `Ctrl+,` — hotkey, theme, OmniLLM URL, max results, startup
- 🖥️ **System Tray** — lives in tray, double-click or hotkey to show
- 🌙 **Dark theme** — Catppuccin Mocha palette
- 🔌 **Plugin system** — drop `OmniLauncher.Plugin.*.dll` in `Plugins/` to extend

## Quick Start

### Build & Run

```bash
# Prerequisites: .NET 8 SDK, Windows 10+
git clone https://github.com/OmniLLM/OmniLauncher
cd OmniLauncher
dotnet run --project src/OmniLauncher
```

The app starts in the system tray. Press **Alt+Space** to show the launcher.

## Keyboard shortcuts

| Key | Action |
|-----|--------|
| `Alt+Space` | Show / hide |
| `↑` / `↓` | Navigate results |
| `Enter` | Execute selected |
| `Escape` | Hide |
| `Ctrl+,` | Open settings |

## Search syntax

| Prefix | Plugin | Example |
|--------|--------|---------|
| *(none)* | App Launcher + Google | `notepad` |
| `g ` | Google | `g rust async book` |
| `yt ` | YouTube | `yt lo-fi beats` |
| `gh ` | GitHub | `gh omnillm` |
| `= ` | Calculator | `= (100 * 1.2) / 3` |
| `f ` | File Search | `f quarterly report` |
| `open ` | File Search | `open Downloads` |
| `cb ` | Clipboard History | `cb api key` |
| `ai ` | OmniLLM AI | `ai explain SOLID principles` |

## Plugin Development

Implement `IPlugin` from `OmniLauncher.Core`:

```csharp
public class MyPlugin : IPlugin
{
    public string Name => "My Plugin";
    public string Description => "Does something cool";
    public string? Keyword => "mp "; // optional prefix filter

    public void Init(PluginInitContext context) { /* setup */ }

    public IList<Result> Query(Query query)
    {
        return new[] { new Result {
            Title = "Hello!",
            SubTitle = "from MyPlugin",
            Action = _ => { /* do thing */ return true; }
        }};
    }
}
```

Build the DLL and drop it in `<install>/Plugins/MyPlugin/`. OmniLauncher auto-discovers on next launch.

## OmniLLM Integration

The AI plugin connects to [OmniLLM](https://github.com/OmniLLM) — a local OpenAI-compatible proxy.

Configure via `Ctrl+,` Settings or `%APPDATA%\OmniLauncher\settings.json`:

```json
{
  "omniLLMUrl": "http://localhost:5000",
  "omniLLMModel": "auto"
}
```

The plugin auto-reads `~/.config/omnillm/api-key` if present.

## License

MIT — see [LICENSE](LICENSE)