using System.Diagnostics;
using OmniLauncher.Core;

namespace OmniLauncher.Plugins;

/// <summary>Run shell commands. Prefix: &gt; — e.g. &gt; ipconfig /all</summary>
public class ShellPlugin : IPlugin
{
    public string  Name        => "Shell";
    public string  Description => "Run a command. Prefix: > ";
    public string? Keyword     => "> ";

    public void Init(PluginInitContext context) { }

    public IList<Result> Query(Query query)
    {
        var cmd = query.Search.Trim();
        if (string.IsNullOrEmpty(cmd)) return Array.Empty<Result>();

        return new[]
        {
            new Result
            {
                Title    = $"▶ {cmd}",
                SubTitle = "Run in Command Prompt",
                Score    = 100,
                Action   = _ =>
                {
                    Process.Start(new ProcessStartInfo("cmd.exe", $"/c start cmd /k {cmd}")
                        { UseShellExecute = true });
                    return true;
                }
            },
            new Result
            {
                Title    = $"▶ {cmd}",
                SubTitle = "Run in PowerShell",
                Score    = 90,
                Action   = _ =>
                {
                    Process.Start(new ProcessStartInfo("powershell.exe", $"-NoExit -Command \"{cmd}\"")
                        { UseShellExecute = true });
                    return true;
                }
            }
        };
    }
}
