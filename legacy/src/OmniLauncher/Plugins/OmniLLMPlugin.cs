using System.Net.Http;
using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using System.Windows;
using OmniLauncher.Core;

namespace OmniLauncher.Plugins;

/// <summary>
/// Sends queries to OmniLLM (OpenAI-compatible proxy at localhost:5000).
/// Prefix: "ai " — e.g. "ai explain async/await in C#"
/// </summary>
public class OmniLLMPlugin : IPlugin
{
    public string Name => "OmniLLM AI";
    public string Description => "Ask AI anything. Prefix: ai ";
    public string? Keyword => "ai ";

    private static readonly HttpClient _http = new() { Timeout = TimeSpan.FromSeconds(30) };
    private string _baseUrl   = "http://localhost:5000";
    private string _model     = "auto";
    private string? _apiKey;
    private int    _maxTokens = 512;

    public void Init(PluginInitContext context)
    {
        var cfgPath = System.IO.Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "OmniLauncher", "settings.json");

        if (System.IO.File.Exists(cfgPath))
        {
            try
            {
                var cfg = JsonDocument.Parse(System.IO.File.ReadAllText(cfgPath)).RootElement;
                if (cfg.TryGetProperty("omniLLMUrl",       out var u)) _baseUrl   = u.GetString() ?? _baseUrl;
                if (cfg.TryGetProperty("omniLLMModel",     out var m)) _model     = m.GetString() ?? _model;
                if (cfg.TryGetProperty("omniLLMApiKey",    out var k)) _apiKey    = k.GetString();
                if (cfg.TryGetProperty("omniLLMMaxTokens", out var t)) _maxTokens = t.GetInt32();
            }
            catch { }
        }

        // Try to read OmniLLM api-key from default location
        _apiKey ??= TryReadApiKey();
    }

    private static string? TryReadApiKey()
    {
        var path = System.IO.Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
            ".config", "omnillm", "api-key");
        return System.IO.File.Exists(path) ? System.IO.File.ReadAllText(path).Trim() : null;
    }

    public IList<Result> Query(Query query)
    {
        var prompt = query.Search.Trim();
        if (string.IsNullOrWhiteSpace(prompt)) return Array.Empty<Result>();

        return new[]
        {
            new Result
            {
                Title    = $"🤖 Ask AI: {(prompt.Length > 60 ? prompt[..60] + "…" : prompt)}",
                SubTitle = $"Send to OmniLLM ({_baseUrl})",
                Score    = 95,
                Action   = _ =>
                {
                    Task.Run(() => AskAndShow(prompt));
                    return false; // keep window open while loading
                }
            }
        };
    }

    private async Task AskAndShow(string prompt)
    {
        try
        {
            var payload = new
            {
                model      = _model,
                messages   = new[] { new { role = "user", content = prompt } },
                max_tokens = _maxTokens,
                stream     = false
            };

            var req = new HttpRequestMessage(HttpMethod.Post, $"{_baseUrl}/v1/chat/completions")
            {
                Content = new StringContent(JsonSerializer.Serialize(payload), Encoding.UTF8, "application/json")
            };
            if (!string.IsNullOrEmpty(_apiKey))
                req.Headers.Authorization = new AuthenticationHeaderValue("Bearer", _apiKey);

            var resp = await _http.SendAsync(req);
            var body = await resp.Content.ReadAsStringAsync();
            var doc  = JsonDocument.Parse(body);
            var text = doc.RootElement
                .GetProperty("choices")[0]
                .GetProperty("message")
                .GetProperty("content")
                .GetString() ?? "(no response)";

            Application.Current.Dispatcher.Invoke(() =>
            {
                var win = new AIResponseWindow(prompt, text);
                win.Show();
            });
        }
        catch (Exception ex)
        {
            Application.Current.Dispatcher.Invoke(() =>
                MessageBox.Show($"OmniLLM error: {ex.Message}", "OmniLauncher", MessageBoxButton.OK, MessageBoxImage.Warning));
        }
    }
}
