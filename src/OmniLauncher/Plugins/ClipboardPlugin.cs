using System.Windows;
using OmniLauncher.Core;

namespace OmniLauncher.Plugins;

/// <summary>
/// Clipboard history. Prefix: "cb " to search past entries.
/// Hooks into clipboard changes via a background listener.
/// Max 50 entries stored in memory.
/// </summary>
public class ClipboardPlugin : IPlugin, IDisposable
{
    public string Name => "Clipboard History";
    public string Description => "Browse and paste clipboard history. Prefix: cb ";
    public string? Keyword => "cb ";

    private readonly List<ClipEntry> _history = new();
    private const int MaxEntries = 50;
    private ClipboardListener? _listener;

    public void Init(PluginInitContext context)
    {
        // Listener must be created on UI thread
        Application.Current.Dispatcher.Invoke(() =>
        {
            _listener = new ClipboardListener();
            _listener.ClipboardChanged += OnClipboardChanged;
        });
    }

    private void OnClipboardChanged(string text)
    {
        if (string.IsNullOrWhiteSpace(text)) return;
        if (_history.Count > 0 && _history[0].Text == text) return; // dedupe

        _history.Insert(0, new ClipEntry(text, DateTime.Now));
        if (_history.Count > MaxEntries) _history.RemoveAt(_history.Count - 1);
    }

    public IList<Result> Query(Query query)
    {
        var term = query.Search.Trim();
        var matches = string.IsNullOrEmpty(term)
            ? _history
            : _history.Where(e => e.Text.Contains(term, StringComparison.OrdinalIgnoreCase)).ToList();

        return matches.Take(8).Select((e, i) => new Result
        {
            Title = e.Text.Length > 80 ? e.Text[..80] + "…" : e.Text,
            SubTitle = $"Copied {FormatAge(e.CopiedAt)} — {e.Text.Length} chars",
            Score = 100 - i,
            Action = _ =>
            {
                Application.Current.Dispatcher.Invoke(() => Clipboard.SetText(e.Text));
                return true;
            }
        }).ToList<Result>();
    }

    private static string FormatAge(DateTime dt)
    {
        var ago = DateTime.Now - dt;
        if (ago.TotalSeconds < 60) return "just now";
        if (ago.TotalMinutes < 60) return $"{(int)ago.TotalMinutes}m ago";
        if (ago.TotalHours < 24) return $"{(int)ago.TotalHours}h ago";
        return dt.ToString("MMM d");
    }

    public void Dispose() => _listener?.Dispose();

    private record ClipEntry(string Text, DateTime CopiedAt);
}