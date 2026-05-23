using System.Reflection;

namespace OmniLauncher.Core;

public class PluginManager
{
    private readonly List<IPlugin> _plugins = new();
    private readonly IPublicAPI _api;

    public PluginManager(IPublicAPI api) => _api = api;

    public void LoadPluginsFromDirectory(string pluginDir)
    {
        if (!Directory.Exists(pluginDir)) return;

        foreach (var dll in Directory.EnumerateFiles(pluginDir, "OmniLauncher.Plugin.*.dll", SearchOption.AllDirectories))
        {
            try
            {
                var asm = Assembly.LoadFrom(dll);
                foreach (var type in asm.GetTypes().Where(t => typeof(IPlugin).IsAssignableFrom(t) && !t.IsAbstract))
                {
                    if (Activator.CreateInstance(type) is IPlugin plugin)
                    {
                        var ctx = new PluginInitContext(Path.GetDirectoryName(dll)!, _api);
                        plugin.Init(ctx);
                        _plugins.Add(plugin);
                    }
                }
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"[PluginManager] Failed to load {dll}: {ex.Message}");
            }
        }
    }

    public void RegisterPlugin(IPlugin plugin, PluginInitContext ctx)
    {
        plugin.Init(ctx);
        _plugins.Add(plugin);
    }

    public IEnumerable<Result> QueryAll(string rawQuery)
    {
        if (string.IsNullOrWhiteSpace(rawQuery)) return Enumerable.Empty<Result>();

        var results = new List<Result>();
        foreach (var plugin in _plugins)
        {
            try
            {
                string actionKeyword = plugin.Keyword ?? string.Empty;
                string search = rawQuery;

                if (!string.IsNullOrEmpty(plugin.Keyword))
                {
                    if (!rawQuery.StartsWith(plugin.Keyword, StringComparison.OrdinalIgnoreCase))
                        continue;
                    search = rawQuery[plugin.Keyword.Length..].TrimStart();
                }

                var query = new Query(rawQuery, search, actionKeyword);
                results.AddRange(plugin.Query(query));
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"[PluginManager] Query error in {plugin.Name}: {ex.Message}");
            }
        }

        return results.OrderByDescending(r => r.Score);
    }

    public IReadOnlyList<IPlugin> Plugins => _plugins.AsReadOnly();
}