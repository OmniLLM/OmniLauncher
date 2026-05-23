using System.Diagnostics;
using OmniLauncher.Core;

namespace OmniLauncher.Plugins;

/// <summary>
/// Prefixes: "g " = Google, "yt " = YouTube, "gh " = GitHub.
/// Plain query with no prefix opens default search.
/// </summary>
public class WebSearchPlugin : IPlugin
{
    public string Name => "Web Search";
    public string Description => "Search the web. Prefix: g, yt, gh";
    public string? Keyword => null;

    private static readonly (string prefix, string name, string url)[] _engines =
    [
        ("g ",  "Google",  "https://www.google.com/search?q={0}"),
        ("yt ", "YouTube", "https://www.youtube.com/results?search_query={0}"),
        ("gh ", "GitHub",  "https://github.com/search?q={0}"),
    ];

    public void Init(PluginInitContext context) { }

    public IList<Result> Query(Query query)
    {
        var q = query.Search.Trim();
        if (string.IsNullOrWhiteSpace(q)) return Array.Empty<Result>();

        var results = new List<Result>();

        foreach (var (prefix, name, urlTemplate) in _engines)
        {
            if (q.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
            {
                var term = Uri.EscapeDataString(q[prefix.Length..].Trim());
                var url  = string.Format(urlTemplate, term);
                results.Add(MakeResult($"Search {name} for '{q[prefix.Length..].Trim()}'", url, 95));
                return results;
            }
        }

        // Default: Google
        results.Add(MakeResult($"Google: {q}", string.Format(_engines[0].url, Uri.EscapeDataString(q)), 30));
        return results;
    }

    private static Result MakeResult(string title, string url, int score) => new()
    {
        Title  = title,
        SubTitle = url,
        Score  = score,
        Action = _ => { Process.Start(new ProcessStartInfo(url) { UseShellExecute = true }); return true; }
    };
}