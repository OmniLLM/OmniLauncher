using System.IO;
using System.Text.Json;

namespace OmniLauncher;

public record AppSettings(
    string Hotkey       = "AltSpace",
    string Theme        = "dark",
    string OmniLLMUrl   = "http://localhost:5000",
    string OmniLLMModel = "auto",
    int    MaxResults   = 8,
    bool   StartOnBoot  = false,
    bool   HideOnLaunch = true)
{
    private static readonly string Path = System.IO.Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "OmniLauncher", "settings.json");

    public static AppSettings Load()
    {
        if (!File.Exists(Path)) return new();
        try { return JsonSerializer.Deserialize<AppSettings>(File.ReadAllText(Path)) ?? new(); }
        catch { return new(); }
    }
}