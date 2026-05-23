# OmniLauncher

A fast, extensible application launcher for Windows — inspired by Flow Launcher.  
Summon it from anywhere with **Alt+Space**.

Built on .NET 8 WPF with a Catppuccin Mocha dark theme.

---

## Features

| | |
|---|---|
| 🚀 **App Launcher** | Searches Start Menu shortcuts (user + common) instantly |
| 🔍 **Web Search** | Google · YouTube · GitHub with typed prefixes |
| 🧮 **Calculator** | Evaluates math expressions, copies result to clipboard |
| 📁 **File Search** | Finds files and folders across your user profile directories |
| 📋 **Clipboard History** | In-memory ring of last 50 clips, searchable and re-pasteable |
| 🤖 **OmniLLM AI** | Sends prompts to your local OmniLLM proxy, shows response in a popup |
| ⚙️ **Settings UI** | Hotkey, theme, OmniLLM URL/model, max results, startup on boot |
| 🖥️ **System Tray** | Runs silently in the tray; double-click or use the hotkey to show |
| 🌙 **Themes** | Catppuccin Mocha (dark) · Catppuccin Latte (light) |
| 🔌 **Plugin system** | Drop `OmniLauncher.Plugin.*.dll` in `Plugins/` to extend |

---

## Quick Start

### Prerequisites

- Windows 10 or later
- [.NET 8 SDK](https://dotnet.microsoft.com/download/dotnet/8.0)

### Build & Run

```bash
git clone https://github.com/OmniLLM/OmniLauncher
cd OmniLauncher
dotnet run --project src/OmniLauncher
```

The app starts minimised to the system tray.  
Press **Alt+Space** to open the launcher.

### Publish a self-contained executable

```bash
dotnet publish src/OmniLauncher -c Release -r win-x64 --self-contained
```

---

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Alt+Space` | Show / hide launcher (default; configurable — see Settings) |
| `↑` / `↓` | Navigate results |
| `Enter` | Execute selected result |
| `Escape` | Hide launcher |
| `Ctrl+,` | Open Settings |

---

## Search Syntax

| Prefix | Plugin | What it does | Example |
|--------|--------|--------------|---------|
| *(none)* | App Launcher | Searches Start Menu shortcuts | `notepad` |
| *(none, no match)* | Web Search | Falls back to Google | `what is SOLID` |
| `g ` | Google | Opens Google search | `g rust async book` |
| `yt ` | YouTube | Opens YouTube search | `yt lo-fi beats` |
| `gh ` | GitHub | Opens GitHub search | `gh omnillm` |
| `=` | Calculator | Evaluates expression, copies to clipboard | `=(100 * 1.2) / 3` |
| `f ` | File Search | Searches files & folders across user dirs | `f quarterly report` |
| `open ` | File Search | Same as `f ` | `open Downloads` |
| `cb ` | Clipboard History | Searches last 50 clipboard entries | `cb api key` |
| `ai ` | OmniLLM AI | Sends prompt to OmniLLM, shows response | `ai explain SOLID principles` |

**Scoring:** App Launcher returns up to 6 results; File Search returns up to 8.  
Results are ordered by score — exact-prefix matches score higher than substring matches.

---

## Configuration

Settings are stored at `%APPDATA%\OmniLauncher\settings.json`.  
Edit via **Ctrl+,** or directly in the file:

```json
{
  "hotkey":       "AltSpace",
  "theme":        "dark",
  "omniLLMUrl":   "http://localhost:5000",
  "omniLLMModel": "auto",
  "omniLLMApiKey": "",
  "maxResults":   8,
  "startOnBoot":  false,
  "hideOnLaunch": true
}
```

| Key | Values | Default | Notes |
|-----|--------|---------|-------|
| `hotkey` | `AltSpace` · `CtrlSpace` · `WinSpace` · `CtrlAltSpace` | `AltSpace` | Requires restart. If the combo is already claimed by another app, OmniLauncher logs a warning and starts without a hotkey — use the tray icon instead. |
| `theme` | `dark` · `light` | `dark` | Requires restart |
| `omniLLMUrl` | Any URL | `http://localhost:5000` | |
| `omniLLMModel` | Model name or `auto` | `auto` | Passed as `model` in the API request |
| `omniLLMApiKey` | API key string | *(empty)* | Falls back to `~/.config/omnillm/api-key` |
| `maxResults` | integer | `8` | Cap on results shown per query |
| `startOnBoot` | bool | `false` | Writes/removes `HKCU\...\Run` registry key |
| `hideOnLaunch` | bool | `true` | When false, window is visible on startup |

### OmniLLM API key resolution

The AI plugin looks for your API key in this order:

1. `omniLLMApiKey` field in `settings.json`
2. `~/.config/omnillm/api-key` file (default OmniLLM install location)
3. No auth header (for proxies that don't require a key)

---

## OmniLLM AI Plugin

The `ai ` prefix sends a non-streaming `POST /v1/chat/completions` request to
your OmniLLM endpoint and displays the response in a dedicated popup window.

The response window shows:

- The original prompt
- The full response text (scrollable)
- A **Copy** button that copies the response to the clipboard

Requests time out after **30 seconds**. Errors are shown in a message box.

---

## Architecture

```
OmniLauncher.sln
├── src/OmniLauncher.Core/        # Shared types — IPlugin, PluginManager, HotkeyManager
│   ├── IPlugin.cs                # IPlugin, IPublicAPI, Query, Result, ActionContext
│   ├── PluginManager.cs          # Loads DLL plugins + routes queries by keyword prefix
│   └── HotkeyManager.cs         # Win32 RegisterHotKey wrapper
└── src/OmniLauncher/             # WPF application
    ├── App.xaml / App.xaml.cs    # Startup, tray init, shutdown mode
    ├── MainWindow.xaml.cs        # Search box, result list, hotkey wiring, IPublicAPI impl
    ├── AppSettings.cs            # settings.json loader (%APPDATA%\OmniLauncher\settings.json)
    ├── TrayIcon.cs               # NotifyIcon with context menu
    ├── Plugins/
    │   ├── AppLauncherPlugin.cs  # Indexes .lnk files from Start Menu at startup
    │   ├── WebSearchPlugin.cs    # g / yt / gh prefixes + Google fallback
    │   ├── CalculatorPlugin.cs   # DataTable.Compute() math evaluator
    │   ├── FileSearchPlugin.cs   # Searches user dirs; also handles direct rooted paths
    │   ├── ClipboardPlugin.cs    # WM_CLIPBOARDUPDATE listener, 50-entry ring buffer
    │   └── OmniLLMPlugin.cs      # HTTP call to OmniLLM, response in AIResponseWindow
    └── Windows/
        ├── SettingsWindow.xaml   # Settings form
        └── AIResponseWindow.xaml # AI response popup with Copy button
```

### How queries are dispatched

1. `MainWindow` calls `PluginManager.QueryAll(rawQuery)` on every keystroke.
2. `PluginManager` iterates all registered plugins.
   - If a plugin has a `Keyword`, only queries that start with that keyword are routed to it; the keyword prefix is stripped before passing to `Query()`.
   - If `Keyword` is `null`, every query is passed (App Launcher, File Search, Web Search all work this way and decide internally whether to return results).
3. Results from all matching plugins are merged and sorted by `Score` descending.
4. Up to `MaxResults` (default 8) are shown.

### Plugin loading order

Built-in plugins are registered first (in `LoadAllPlugins`):

1. `AppLauncherPlugin`
2. `WebSearchPlugin`
3. `CalculatorPlugin`
4. `FileSearchPlugin`
5. `ClipboardPlugin`
6. `OmniLLMPlugin`

External plugins from `<install>/Plugins/` are loaded last via reflection.

---

## Plugin Development

Implement `IPlugin` from `OmniLauncher.Core`:

```csharp
// MyPlugin.cs
using OmniLauncher.Core;

public class MyPlugin : IPlugin
{
    public string  Name        => "My Plugin";
    public string  Description => "Does something cool";
    public string? Keyword     => "mp "; // null to match all queries

    public void Init(PluginInitContext context)
    {
        // context.PluginDirectory — absolute path to your plugin folder
        // context.API             — IPublicAPI for ChangeQuery / HideMainWindow / ShowMainWindow
    }

    public IList<Result> Query(Query query)
    {
        // query.RawQuery      — full string typed by user
        // query.Search        — raw query minus the keyword prefix (if Keyword is set)
        // query.ActionKeyword — your Keyword value (or empty string)

        return new[]
        {
            new Result
            {
                Title    = "Hello from MyPlugin",
                SubTitle = query.Search,
                Score    = 80,          // higher = higher up in the list
                IcoPath  = null,        // optional icon path
                Action   = ctx =>
                {
                    // ctx.SpecialKeyState — true when Ctrl is held on Enter
                    // return true  → hide the launcher after execution
                    // return false → keep the launcher open
                    return true;
                }
            }
        };
    }
}
```

### Project file

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0-windows</TargetFramework>
    <AssemblyName>OmniLauncher.Plugin.MyPlugin</AssemblyName>
  </PropertyGroup>
  <ItemGroup>
    <ProjectReference Include="path/to/OmniLauncher.Core.csproj" />
  </ItemGroup>
</Project>
```

### Deployment

Build the DLL and place it in `<install>/Plugins/MyPlugin/OmniLauncher.Plugin.MyPlugin.dll`.  
OmniLauncher auto-discovers any DLL matching `OmniLauncher.Plugin.*.dll` recursively under `Plugins/` on next launch.

### IPublicAPI

| Method | Description |
|--------|-------------|
| `ChangeQuery(string query, bool requery)` | Replaces the text in the search box. Pass `requery: true` to immediately re-run the query. |
| `HideMainWindow()` | Hides the launcher window. |
| `ShowMainWindow()` | Shows and focuses the launcher window. |

---

## License

MIT — see [LICENSE](LICENSE)
