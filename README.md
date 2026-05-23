# OmniLauncher

A fast, extensible application launcher for Windows — inspired by Flow.Launcher.
Summon it anywhere with **Alt+Space**.

![OmniLauncher dark theme](docs/screenshot.png)

## Features

- 🚀 **Instant app launch** — searches Start Menu shortcuts
- 🔍 **Web search** — `g ` Google · `yt ` YouTube · `gh ` GitHub
- 🧮 **Calculator** — prefix `=` evaluates any expression, copies result
- 🔌 **Plugin system** — drop a `OmniLauncher.Plugin.*.dll` in the `Plugins/` folder
- 🌙 **Dark theme** — Catppuccin Mocha palette
- ⌨️ **Global hotkey** — `Alt+Space` (configurable)

## Quick Start

### Build & Run

```bash
# Prerequisites: .NET 8 SDK, Windows 10+
git clone https://github.com/OmniLLM/OmniLauncher
cd OmniLauncher
dotnet build src/OmniLauncher/OmniLauncher.csproj -c Release
dotnet run --project src/OmniLauncher
```

The window starts hidden. Press **Alt+Space** to toggle it.

### Keyboard shortcuts

| Key | Action |
|-----|--------|
| `Alt+Space` | Show / hide |
| `↑` / `↓` | Navigate results |
| `Enter` | Execute selected |
| `Escape` | Hide |

### Search syntax

| Prefix | Action |
|--------|--------|
| *(none)* | App search + Google |
| `g ` | Google search |
| `yt ` | YouTube search |
| `gh ` | GitHub search |
| `= ` | Calculator (e.g. `= 2+2*3`) |

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
        return new[] { new Result { Title = "Hello!", Action = _ => { /* do thing */ return true; } } };
    }
}
```

Build the DLL and drop it in `<install>/Plugins/YourPlugin/`. OmniLauncher auto-discovers it on next launch.

## Roadmap

- [ ] Settings UI (hotkey customization, theme picker)
- [ ] File search plugin
- [ ] Clipboard history plugin
- [ ] OmniLLM AI chat plugin (query local LLMs via OmniLLM proxy)
- [ ] Plugin marketplace

## License

MIT — see [LICENSE](LICENSE)