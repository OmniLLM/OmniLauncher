using System.Diagnostics;
using System.IO;
using OmniLauncher.Core;

namespace OmniLauncher.Plugins;

/// <summary>
/// Searches files and folders. Prefix: "f " or "open "
/// Also handles direct path input (e.g. C:\Users\...)
/// </summary>
public class FileSearchPlugin : IPlugin
{
    public string Name => "File Search";
    public string Description => "Find files and folders. Prefix: f  or open ";
    public string? Keyword => null;

    private static readonly string[] _searchRoots =
    [
        Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
        Environment.GetFolderPath(Environment.SpecialFolder.Desktop),
        Environment.GetFolderPath(Environment.SpecialFolder.MyDocuments),
        Environment.GetFolderPath(Environment.SpecialFolder.MyPictures),
        Environment.GetFolderPath(Environment.SpecialFolder.MyMusic),
        Environment.GetFolderPath(Environment.SpecialFolder.MyVideos),
    ];

    public void Init(PluginInitContext context) { }

    public IList<Result> Query(Query query)
    {
        var raw = query.Search.Trim();

        // Strip prefix
        string? term = null;
        if (raw.StartsWith("f ", StringComparison.OrdinalIgnoreCase))
            term = raw[2..].Trim();
        else if (raw.StartsWith("open ", StringComparison.OrdinalIgnoreCase))
            term = raw[5..].Trim();
        else if (Path.IsPathRooted(raw))
            term = raw; // Direct path
        else
            return Array.Empty<Result>();

        if (string.IsNullOrWhiteSpace(term)) return Array.Empty<Result>();

        // Direct path that exists
        if (Path.IsPathRooted(term) && (File.Exists(term) || Directory.Exists(term)))
            return new[] { MakeResult(term, term, 100) };

        var results = new List<Result>();
        foreach (var root in _searchRoots.Where(Directory.Exists))
        {
            try
            {
                // Directories
                foreach (var dir in Directory.EnumerateDirectories(root, $"*{term}*", SearchOption.AllDirectories).Take(3))
                    results.Add(MakeResult(Path.GetFileName(dir), dir, 75));

                // Files
                foreach (var file in Directory.EnumerateFiles(root, $"*{term}*", SearchOption.AllDirectories).Take(5))
                    results.Add(MakeResult(Path.GetFileName(file), file, 70));
            }
            catch { /* skip inaccessible */ }

            if (results.Count >= 8) break;
        }

        return results.Take(8).ToList();
    }

    private static Result MakeResult(string name, string path, int score) => new()
    {
        Title = name,
        SubTitle = path,
        Score = score,
        Action = _ =>
        {
            Process.Start(new ProcessStartInfo(path) { UseShellExecute = true });
            return true;
        }
    };
}