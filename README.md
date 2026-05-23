# OmniLauncher

A fast, extensible Windows launcher inspired by Flow.Launcher.
By default it starts in the system tray and opens with **Alt+Space**.

## Features

- App launcher via Start Menu shortcut indexing
- Web search with `g `, `yt `, and `gh ` prefixes
- Calculator with `=` prefix and clipboard copy on execute
- File and folder search with `f `, `open `, or direct rooted paths
- Clipboard history with `cb ` prefix
- OmniLLM AI integration with `ai ` prefix
- Settings window on `Ctrl+,`
- System tray icon with show, settings, and exit actions
- Plugin loading from the `Plugins/` directory
- Catppuccin Mocha dark theme

## Quick Start

### Build & Run

```powershell
# Prerequisites: Windows 10+ and .NET 8 SDK
git clone https://github.com/OmniLLM/OmniLauncher
cd OmniLauncher
dotnet build .\src\OmniLauncher\OmniLauncher.csproj
dotnet run --project .\src\OmniLauncher\OmniLauncher.csproj
```

By default the app starts hidden in the system tray. Use the configured hotkey or double-click the tray icon to show the launcher.
If the selected global hotkey is already taken, the app stays running and can still be opened from the tray icon.

## Keyboard shortcuts

| Key | Action |
|-----|--------|
| `Alt+Space` | Default show / hide hotkey |
| `↑` / `↓` | Navigate results |
| `Enter` | Execute selected result |
| `Escape` | Hide launcher |
| `Ctrl+,` | Open settings |

Available hotkey options in settings:

- `Alt+Space` (default)
- `Ctrl+Space`
- `Win+Space`
- `Ctrl+Alt+Space`

## Search syntax

| Prefix | Plugin | Example |
|--------|--------|---------|
| *(none)* | App Launcher + Google fallback | `notepad` |
| `g ` | Google | `g rust async book` |
| `yt ` | YouTube | `yt lo-fi beats` |
| `gh ` | GitHub | `gh omnillm` |
| `= ` | Calculator | `= (100 * 1.2) / 3` |
| `f ` | File Search | `f quarterly report` |
| `open ` | File Search | `open Downloads` |
| `cb ` | Clipboard History | `cb api key` |
| `ai ` | OmniLLM AI | `ai explain SOLID principles` |

## Settings

Settings are stored at `%APPDATA%\OmniLauncher\settings.json`.

Example:

```json
{
  "hotkey": "AltSpace",
  "theme": "dark",
  "omniLLMUrl": "http://localhost:5000",
  "omniLLMModel": "auto",
  "maxResults": 8,
  "startOnBoot": false,
  "hideOnLaunch": true
}
```

Notes:

- The settings UI exposes a light theme option, but the app currently ships with `Themes/Dark.xaml` loaded by default.
- `Start on Windows login` writes the launcher to `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run`.

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

Build the DLL and drop it in `Plugins/`. OmniLauncher loads plugins from `<app base directory>\Plugins` on startup.

## OmniLLM Integration

The AI plugin connects to [OmniLLM](https://github.com/OmniLLM) — a local OpenAI-compatible proxy.

Configure via `Ctrl+,` settings or `%APPDATA%\OmniLauncher\settings.json`:

```json
{
  "hotkey": "AltSpace",
  "omniLLMUrl": "http://localhost:5000",
  "omniLLMModel": "auto",
  "maxResults": 8
}
```

The plugin auto-reads `~/.config/omnillm/api-key` if present.

## License

MIT — see [LICENSE](LICENSE)
