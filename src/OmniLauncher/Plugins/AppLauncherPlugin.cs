using System.Diagnostics;
using System.IO;
using OmniLauncher.Core;

namespace OmniLauncher.Plugins;

/// <summary>Searches installed apps via Start Menu shortcuts and common paths.</summary>
public class AppLauncherPlugin : IPlugin
{
    public string Name => "App Launcher";
    public string Description => "Launch installed applications";
    public string? Keyword => null; // participates in all queries

    private List<AppEntry> _apps = new();

    public void Init(PluginInitContext context) => IndexApps();

    private void IndexApps()
    {
        var dirs = new[]
        {
            Environment.GetFolderPath(Environment.SpecialFolder.StartMenu),
            Environment.GetFolderPath(Environment.SpecialFolder.CommonStartMenu),
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                "Microsoft\Windows\Start Menu"),
        };

        foreach (var dir in dirs.Where(Directory.Exists))
        {
            foreach (var lnk in Directory.EnumerateFiles(dir, "*.lnk", SearchOption.AllDirectories))
            {
                var name = Path.GetFileNameWithoutExtension(lnk);
                _apps.Add(new AppEntry(name, lnk));
            }
        }
    }

    public IList<Result> Query(Query query)
    {
        if (string.IsNullOrWhiteSpace(query.Search)) return Array.Empty<Result>();

        return _apps
            .Where(a => a.Name.Contains(query.Search, StringComparison.OrdinalIgnoreCase))
            .OrderBy(a => a.Name)
            .Take(6)
            .Select(a => new Result
            {
                Title = a.Name,
                SubTitle = a.Path,
                Score = a.Name.StartsWith(query.Search, StringComparison.OrdinalIgnoreCase) ? 90 : 60,
                Action = _ =>
                {
                    Process.Start(new ProcessStartInfo(a.Path) { UseShellExecute = true });
                    return true;
                }
            })
            .ToList();
    }

    private record AppEntry(string Name, string Path);
}