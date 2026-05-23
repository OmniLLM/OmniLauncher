using System.Diagnostics;
using OmniLauncher.Core;

namespace OmniLauncher.Plugins;

/// <summary>Kill running processes by name. Prefix: kill </summary>
public class ProcessKillerPlugin : IPlugin
{
    public string  Name        => "Process Killer";
    public string  Description => "Kill a running process. Prefix: kill ";
    public string? Keyword     => "kill ";

    public void Init(PluginInitContext context) { }

    public IList<Result> Query(Query query)
    {
        var search = query.Search.ToLowerInvariant().Trim();
        if (string.IsNullOrEmpty(search)) return Array.Empty<Result>();

        return Process.GetProcesses()
            .Where(p => p.ProcessName.ToLowerInvariant().Contains(search))
            .OrderBy(p => p.ProcessName)
            .Take(8)
            .Select(p =>
            {
                var captured = p;
                return new Result
                {
                    Title    = $"✕ {captured.ProcessName}",
                    SubTitle = $"PID {captured.Id} · {TryMainWindowTitle(captured)}",
                    Score    = captured.ProcessName.ToLowerInvariant().StartsWith(search) ? 90 : 70,
                    Action   = _ =>
                    {
                        try { captured.Kill(entireProcessTree: true); } catch { }
                        return true;
                    }
                };
            })
            .ToList();
    }

    private static string TryMainWindowTitle(Process p)
    {
        try { return string.IsNullOrEmpty(p.MainWindowTitle) ? "" : p.MainWindowTitle; } catch { return ""; }
    }
}
