using System.Runtime.InteropServices;
using System.Text;
using System.Diagnostics;
using OmniLauncher.Core;

namespace OmniLauncher.Plugins;

/// <summary>Switch to open windows. Prefix: ww </summary>
public class WindowWalkerPlugin : IPlugin
{
    public string  Name        => "Window Walker";
    public string  Description => "Switch to an open window. Prefix: ww ";
    public string? Keyword     => "ww ";

    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] private static extern int  GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
    [DllImport("user32.dll")] private static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] private static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

    private delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    public void Init(PluginInitContext context) { }

    public IList<Result> Query(Query query)
    {
        var search = query.Search.ToLowerInvariant();
        var results = new List<Result>();

        EnumWindows((hWnd, _) =>
        {
            if (!IsWindowVisible(hWnd)) return true;
            var sb = new StringBuilder(256);
            GetWindowText(hWnd, sb, 256);
            var title = sb.ToString();
            if (string.IsNullOrWhiteSpace(title)) return true;
            if (!string.IsNullOrEmpty(search) && !title.ToLowerInvariant().Contains(search)) return true;

            GetWindowThreadProcessId(hWnd, out var pid);
            string procName = "";
            try { procName = Process.GetProcessById((int)pid).ProcessName; } catch { }

            var captured = hWnd;
            results.Add(new Result
            {
                Title    = title,
                SubTitle = procName,
                Score    = title.ToLowerInvariant().StartsWith(search) ? 90 : 70,
                Action   = _ =>
                {
                    ShowWindow(captured, 9 /* SW_RESTORE */);
                    SetForegroundWindow(captured);
                    return true;
                }
            });
            return true;
        }, IntPtr.Zero);

        return results.OrderByDescending(r => r.Score).Take(8).ToList();
    }
}
