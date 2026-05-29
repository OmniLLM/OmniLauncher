using System.Reflection;
using System.Runtime.Loader;
using System.Text.Json;
using System.IO;
using System.Net.Http;
using Flow.Launcher.Plugin;
using Flow.Launcher.Plugin.SharedModels;

try
{
    if (args.Length < 3)
    {
        WriteError("Usage: OmniFlowHost <plugin-dir> <query|execute> <payload-json>");
        return 2;
    }

    var pluginDir = Path.GetFullPath(args[0]);
    var op = args[1];
    var payload = JsonSerializer.Deserialize<JsonElement>(args[2]);
    var manifest = ReadFlowManifest(pluginDir);
    var pluginDll = ResolvePluginDll(pluginDir, manifest.ExecuteFileName);
    var host = FlowPluginHost.Load(pluginDir, pluginDll, manifest);

    if (op == "query")
    {
        var queryText = payload.TryGetProperty("query", out var q) ? q.GetString() ?? string.Empty : string.Empty;
        var results = await host.QueryAsync(queryText);
        WriteJson(new { results });
        return 0;
    }

    if (op == "execute")
    {
        var queryText = payload.TryGetProperty("query", out var q) ? q.GetString() ?? string.Empty : string.Empty;
        var index = payload.TryGetProperty("index", out var i) ? i.GetInt32() : -1;
        var output = await host.ExecuteAsync(queryText, index);
        WriteJson(new { output });
        return 0;
    }

    WriteError($"Unsupported operation: {op}");
    return 2;
}
catch (Exception ex)
{
    WriteError(ex.ToString());
    return 1;
}

static FlowManifest ReadFlowManifest(string pluginDir)
{
    var path = Path.Combine(pluginDir, "flow.plugin.json");
    if (!File.Exists(path)) path = Path.Combine(pluginDir, "plugin.json");
    var json = File.ReadAllText(path);
    return JsonSerializer.Deserialize<FlowManifest>(json, new JsonSerializerOptions { PropertyNameCaseInsensitive = true })
        ?? throw new InvalidOperationException("Failed to parse Flow plugin manifest.");
}

static string ResolvePluginDll(string pluginDir, string executeFileName)
{
    if (string.IsNullOrWhiteSpace(executeFileName))
        throw new InvalidOperationException("Flow manifest is missing ExecuteFileName.");

    var direct = Path.IsPathRooted(executeFileName)
        ? executeFileName
        : Path.Combine(pluginDir, executeFileName);
    if (File.Exists(direct)) return Path.GetFullPath(direct);

    var fileName = Path.GetFileName(executeFileName);
    var candidates = Directory.EnumerateFiles(pluginDir, fileName, SearchOption.AllDirectories)
        .Where(p => !p.Contains($"{Path.DirectorySeparatorChar}obj{Path.DirectorySeparatorChar}", StringComparison.OrdinalIgnoreCase))
        .OrderByDescending(p => p.Contains($"{Path.DirectorySeparatorChar}Output{Path.DirectorySeparatorChar}", StringComparison.OrdinalIgnoreCase))
        .ThenByDescending(p => p.Contains($"{Path.DirectorySeparatorChar}Release{Path.DirectorySeparatorChar}", StringComparison.OrdinalIgnoreCase))
        .ThenBy(p => p.Length)
        .ToList();

    return candidates.FirstOrDefault()
        ?? throw new FileNotFoundException($"Could not find built Flow plugin DLL '{executeFileName}'. Run dotnet build for the plugin.");
}

static void WriteJson(object value) => Console.WriteLine(JsonSerializer.Serialize(value));

static void WriteError(string message) => WriteJson(new { error = message });

sealed class FlowManifest
{
    public string? ID { get; set; }
    public string? Name { get; set; }
    public string? Author { get; set; }
    public string? Version { get; set; }
    public string? Language { get; set; }
    public string? Description { get; set; }
    public string? Website { get; set; }
    public string? ActionKeyword { get; set; }
    public List<string>? ActionKeywords { get; set; }
    public string ExecuteFileName { get; set; } = string.Empty;
    public string? IcoPath { get; set; }
}

sealed class FlowPluginHost
{
    private readonly string pluginDir;
    private readonly FlowManifest manifest;
    private readonly object plugin;
    private readonly StubPublicApi api;

    private FlowPluginHost(string pluginDir, FlowManifest manifest, object plugin, StubPublicApi api)
    {
        this.pluginDir = pluginDir;
        this.manifest = manifest;
        this.plugin = plugin;
        this.api = api;
    }

    public static FlowPluginHost Load(string pluginDir, string pluginDll, FlowManifest manifest)
    {
        var resolver = new AssemblyDependencyResolver(pluginDll);
        AssemblyLoadContext.Default.Resolving += (_, name) =>
        {
            var path = resolver.ResolveAssemblyToPath(name);
            return path is null ? null : Assembly.LoadFrom(path);
        };

        var assembly = Assembly.LoadFrom(pluginDll);
        var pluginType = assembly.GetTypes().FirstOrDefault(t =>
            !t.IsAbstract && !t.IsInterface &&
            (typeof(IPlugin).IsAssignableFrom(t) || typeof(IAsyncPlugin).IsAssignableFrom(t)));
        if (pluginType is null)
            throw new InvalidOperationException($"No IPlugin/IAsyncPlugin implementation found in {pluginDll}.");

        var plugin = Activator.CreateInstance(pluginType)
            ?? throw new InvalidOperationException($"Failed to create plugin type {pluginType.FullName}.");

        var api = new StubPublicApi(pluginDir);
        var metadata = new PluginMetadata
        {
            ID = manifest.ID ?? manifest.Name ?? Path.GetFileName(pluginDir),
            Name = manifest.Name ?? manifest.ID ?? Path.GetFileName(pluginDir),
            Author = manifest.Author ?? string.Empty,
            Version = manifest.Version ?? "0.0.0",
            Language = manifest.Language ?? string.Empty,
            Description = manifest.Description ?? string.Empty,
            Website = manifest.Website ?? string.Empty,
            ExecuteFileName = pluginDll,
            ActionKeyword = manifest.ActionKeyword ?? "*",
            ActionKeywords = manifest.ActionKeywords ?? new List<string>(),
            IcoPath = manifest.IcoPath ?? string.Empty,
        };
        SetMetadataPath(metadata, "ExecuteFilePath", pluginDll);
        SetMetadataPath(metadata, "PluginDirectory", Path.GetDirectoryName(pluginDll) ?? pluginDir);

        var context = new PluginInitContext(metadata, api);
        if (plugin is IAsyncPlugin asyncPlugin)
        {
            asyncPlugin.InitAsync(context).GetAwaiter().GetResult();
        }
        else if (plugin is IPlugin syncPlugin)
        {
            syncPlugin.Init(context);
        }

        return new FlowPluginHost(pluginDir, manifest, plugin, api);
    }

    private static void SetMetadataPath(PluginMetadata metadata, string propertyName, string value)
    {
        var property = typeof(PluginMetadata).GetProperty(
            propertyName,
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic);
        if (property?.GetSetMethod(true) is { } setter)
        {
            setter.Invoke(metadata, new object[] { value });
            return;
        }

        var field = typeof(PluginMetadata).GetField(
            $"<{propertyName}>k__BackingField",
            BindingFlags.Instance | BindingFlags.NonPublic);
        field?.SetValue(metadata, value);
    }

    public async Task<List<FlowResultDto>> QueryAsync(string search)
    {
        api.ClearMessages();
        var query = BuildQuery(search);
        List<Result> results;
        if (plugin is IAsyncPlugin asyncPlugin)
        {
            results = await asyncPlugin.QueryAsync(query, CancellationToken.None);
        }
        else
        {
            results = ((IPlugin)plugin).Query(query);
        }

        return results.Select((r, i) => new FlowResultDto
        {
            Index = i,
            Title = r.Title ?? string.Empty,
            SubTitle = r.SubTitle,
            IcoPath = ResolveIcon(r.IcoPath),
            Score = r.Score,
            HasAction = r.Action is not null,
        }).ToList();
    }

    public async Task<string> ExecuteAsync(string search, int index)
    {
        api.ClearMessages();
        var query = BuildQuery(search);
        List<Result> results;
        if (plugin is IAsyncPlugin asyncPlugin)
        {
            results = await asyncPlugin.QueryAsync(query, CancellationToken.None);
        }
        else
        {
            results = ((IPlugin)plugin).Query(query);
        }

        if (index < 0 || index >= results.Count) return "Result action not found.";
        var action = results[index].Action;
        if (action is null) return "No action attached to result.";
        var handled = action(new ActionContext { SpecialKeyState = new SpecialKeyState() });
        var messages = api.DrainMessages();
        if (messages.Count > 0) return string.Join("\n", messages);
        return handled ? "Action executed." : "Action returned false.";
    }

    private Query BuildQuery(string search)
    {
        var keyword = manifest.ActionKeyword ?? "*";
        var terms = search.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        var raw = keyword == "*" ? search : $"{keyword} {search}".Trim();
        return new Query(raw, search, terms, terms, keyword);
    }

    private string? ResolveIcon(string? icoPath)
    {
        if (string.IsNullOrWhiteSpace(icoPath)) return manifest.IcoPath;
        if (Path.IsPathRooted(icoPath)) return icoPath;
        var abs = Path.Combine(pluginDir, icoPath);
        return File.Exists(abs) ? abs : icoPath;
    }
}

sealed class FlowResultDto
{
    public int Index { get; set; }
    public string Title { get; set; } = string.Empty;
    public string? SubTitle { get; set; }
    public string? IcoPath { get; set; }
    public int Score { get; set; }
    public bool HasAction { get; set; }
}

sealed class StubPublicApi : IPublicAPI
{
    private readonly string pluginDir;
    private readonly List<string> messages = new();

    public StubPublicApi(string pluginDir) => this.pluginDir = pluginDir;

    public event FlowLauncherGlobalKeyboardEventHandler? GlobalKeyboardEvent { add { } remove { } }

    public void ClearMessages() => messages.Clear();
    public List<string> DrainMessages() => messages.ToList();

    public void ChangeQuery(string query, bool requery = false) => messages.Add($"Change query: {query}");
    public void RestartApp() => messages.Add("Restart requested.");
    public void ShellRun(string cmd, string parameters = "")
    {
        try { System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(cmd, parameters) { UseShellExecute = true }); }
        catch (Exception ex) { messages.Add($"ShellRun failed: {ex.Message}"); }
    }
    public void CopyToClipboard(string text) { try { Clipboard.SetText(text); messages.Add("Copied to clipboard."); } catch { messages.Add(text); } }
    public void SaveAppAllSettings() { }
    public void SavePluginSettings() { }
    public Task ReloadAllPluginData() => Task.CompletedTask;
    public void CheckForNewUpdate() { }
    public void ShowMsgError(string title, string subTitle) => messages.Add($"Error: {title} {subTitle}".Trim());
    public void ShowMainWindow() { }
    public void ShowMsg(string title, string subTitle = "", string iconPath = "") => messages.Add($"{title} {subTitle}".Trim());
    public void ShowMsg(string title, string subTitle, string iconPath, bool useMainWindowAsOwner) => ShowMsg(title, subTitle, iconPath);
    public void OpenSettingDialog() => messages.Add("Settings dialog is not available in OmniLauncher.");
    public string GetTranslation(string key) => key;
    public List<PluginPair> GetAllPlugins() => new();
    public void RegisterGlobalKeyboardCallback(Func<int, int, SpecialKeyState, bool> callback) { }
    public void RemoveGlobalKeyboardCallback(Func<int, int, SpecialKeyState, bool> callback) { }
    public MatchResult FuzzySearch(string query, string stringToCompare) => new(stringToCompare.Contains(query, StringComparison.OrdinalIgnoreCase), SearchPrecisionScore.Regular, new List<int>(), 0);
    public Task<string> HttpGetStringAsync(string url, CancellationToken token = default) => new HttpClient().GetStringAsync(url, token);
    public Task<Stream> HttpGetStreamAsync(string url, CancellationToken token = default) => new HttpClient().GetStreamAsync(url, token);
    public async Task HttpDownloadAsync(string url, string filePath, CancellationToken token = default)
    {
        await using var input = await HttpGetStreamAsync(url, token);
        await using var output = File.Create(filePath);
        await input.CopyToAsync(output, token);
    }
    public void AddActionKeyword(string pluginId, string newActionKeyword) { }
    public void RemoveActionKeyword(string pluginId, string oldActionKeyword) { }
    public void LogDebug(string className, string message, string methodName = "") { }
    public void LogInfo(string className, string message, string methodName = "") { }
    public void LogWarn(string className, string message, string methodName = "") { }
    public void LogException(string className, string message, Exception exception, string methodName = "") { messages.Add($"{message}: {exception.Message}"); }
    public T LoadSettingJsonStorage<T>() where T : new()
    {
        var path = Path.Combine(pluginDir, $"{typeof(T).Name}.json");
        if (File.Exists(path))
        {
            try { return JsonSerializer.Deserialize<T>(File.ReadAllText(path)) ?? new T(); } catch { }
        }
        return new T();
    }
    public void SaveSettingJsonStorage<T>() where T : new() { }
    public void OpenDirectory(string path, string fileNameOrFilePath = "") => ShellRun(Path.Combine(path, fileNameOrFilePath));
    public void OpenUrl(string url, bool? inPrivate = null) => ShellRun(url);
}
