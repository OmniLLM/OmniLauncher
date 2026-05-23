using System.Diagnostics;
using OmniLauncher.Core;

namespace OmniLauncher.Plugins;

/// <summary>System actions: lock, sleep, restart, shutdown, empty trash, screen off.</summary>
public class SystemCommandsPlugin : IPlugin
{
    public string  Name        => "System Commands";
    public string  Description => "lock · sleep · restart · shutdown · trash · screenoff";
    public string? Keyword     => null;

    private static readonly (string key, string title, string sub, Action action)[] Commands =
    {
        ("lock",      "🔒 Lock",          "Lock the screen",         () => LockWorkStation()),
        ("sleep",     "💤 Sleep",          "Put computer to sleep",   () => Run("rundll32", "powrprof.dll,SetSuspendState 0,1,0")),
        ("restart",   "🔄 Restart",        "Restart Windows",         () => Run("shutdown", "/r /t 5 /c \"Restarting via OmniLauncher\"")),
        ("shutdown",  "⏻ Shut Down",       "Shut down Windows",       () => Run("shutdown", "/s /t 5 /c \"Shutting down via OmniLauncher\"")),
        ("trash",     "🗑 Empty Trash",     "Empty the Recycle Bin",   () => EmptyTrash()),
        ("screenoff", "🖥 Screen Off",      "Turn off the monitor",    () => Run("nircmd", "monitor off")),
    };

    [System.Runtime.InteropServices.DllImport("user32.dll")]
    private static extern bool LockWorkStation();

    private static void Run(string exe, string args) =>
        Process.Start(new ProcessStartInfo(exe, args) { UseShellExecute = true });

    private static void EmptyTrash() =>
        Microsoft.VisualBasic.FileIO.FileSystem.DeleteDirectory(
            Environment.GetFolderPath(Environment.SpecialFolder.RecycleBin),
            Microsoft.VisualBasic.FileIO.UIOption.AllDialogs,
            Microsoft.VisualBasic.FileIO.RecycleOption.DeletePermanently);

    public void Init(PluginInitContext context) { }

    public IList<Result> Query(Query query)
    {
        var q = query.Search.ToLowerInvariant().Trim();
        var results = new List<Result>();
        foreach (var (key, title, sub, action) in Commands)
        {
            if (!key.StartsWith(q) && !q.StartsWith(key)) continue;
            var captured = action;
            results.Add(new Result { Title = title, SubTitle = sub, Score = 95, Action = _ => { captured(); return true; } });
        }
        return results;
    }
}
