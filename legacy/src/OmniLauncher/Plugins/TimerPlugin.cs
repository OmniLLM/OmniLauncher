using System.Windows;
using OmniLauncher.Core;

namespace OmniLauncher.Plugins;

/// <summary>
/// Set a toast notification timer. Prefix: timer — e.g. timer 5m coffee, timer 1h30m meeting
/// </summary>
public class TimerPlugin : IPlugin
{
    public string  Name        => "Timer";
    public string  Description => "Set a timer. Prefix: timer — e.g. timer 5m coffee";
    public string? Keyword     => "timer ";

    public void Init(PluginInitContext context) { }

    public IList<Result> Query(Query query)
    {
        var input = query.Search.Trim();
        if (string.IsNullOrEmpty(input)) return Array.Empty<Result>();

        if (!TryParseDuration(input, out var duration, out var label))
            return Array.Empty<Result>();

        var displayLabel = string.IsNullOrEmpty(label) ? "Timer" : label;

        return new[]
        {
            new Result
            {
                Title    = $"⏱ {displayLabel} — {FormatDuration(duration)}",
                SubTitle = $"Shows a notification after {FormatDuration(duration)}",
                Score    = 100,
                Action   = _ =>
                {
                    var d = duration;
                    var lbl = displayLabel;
                    Task.Delay(d).ContinueWith(_ =>
                        Application.Current.Dispatcher.Invoke(() =>
                            MessageBox.Show($"⏱ {lbl}", "OmniLauncher Timer",
                                MessageBoxButton.OK, MessageBoxImage.Information)));
                    return true;
                }
            }
        };
    }

    private static bool TryParseDuration(string input, out TimeSpan duration, out string label)
    {
        duration = TimeSpan.Zero;
        label    = "";

        // Extract duration tokens (1h30m, 5m, 90s, 2h, etc.)
        var m = System.Text.RegularExpressions.Regex.Match(input,
            @"^((?:\d+h)?(?:\d+m)?(?:\d+s)?)\s*(.*)?$",
            System.Text.RegularExpressions.RegexOptions.IgnoreCase);

        if (!m.Success || string.IsNullOrEmpty(m.Groups[1].Value)) return false;

        var parts = m.Groups[1].Value.ToLowerInvariant();
        label = m.Groups[2].Value.Trim();

        int h = 0, min = 0, sec = 0;
        var hm = System.Text.RegularExpressions.Regex.Match(parts, @"(\d+)h"); if (hm.Success) h   = int.Parse(hm.Groups[1].Value);
        var mm = System.Text.RegularExpressions.Regex.Match(parts, @"(\d+)m"); if (mm.Success) min = int.Parse(mm.Groups[1].Value);
        var sm = System.Text.RegularExpressions.Regex.Match(parts, @"(\d+)s"); if (sm.Success) sec = int.Parse(sm.Groups[1].Value);

        if (h == 0 && min == 0 && sec == 0) return false;
        duration = new TimeSpan(h, min, sec);
        return true;
    }

    private static string FormatDuration(TimeSpan t)
    {
        if (t.TotalSeconds < 60)  return $"{t.Seconds}s";
        if (t.TotalMinutes < 60)  return t.Seconds > 0 ? $"{t.Minutes}m {t.Seconds}s" : $"{t.Minutes}m";
        return t.Minutes > 0 ? $"{(int)t.TotalHours}h {t.Minutes}m" : $"{(int)t.TotalHours}h";
    }
}
