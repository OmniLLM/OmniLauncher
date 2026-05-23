using System.IO;
using System.Text.Json;

namespace OmniLauncher;

public record AppSettings(
    string Hotkey           = "Alt+Space",
    string Theme            = "dark",
    string Provider         = "omnillm",
    string OmniLLMUrl       = "http://localhost:5000",
    string OmniLLMModel     = "auto",
    string OmniLLMApiKey    = "",
    int    OmniLLMMaxTokens = 512,
    int    MaxResults       = 8,
    bool   StartOnBoot      = false,
    bool   HideOnLaunch     = true)
{
    private static readonly string Path = System.IO.Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "OmniLauncher", "settings.json");

    public static AppSettings Load()
    {
        if (!File.Exists(Path)) return new();
        try
        {
            var s = JsonSerializer.Deserialize<AppSettings>(File.ReadAllText(Path)) ?? new();
            // Normalise legacy hotkey strings
            return s with { Hotkey = NormaliseHotkey(s.Hotkey) };
        }
        catch { return new(); }
    }

    /// <summary>Converts legacy values (AltSpace, CtrlSpace …) to the modern 'Mod+Key' format.</summary>
    private static string NormaliseHotkey(string raw) => raw switch
    {
        "AltSpace"     => "Alt+Space",
        "CtrlSpace"    => "Ctrl+Space",
        "WinSpace"     => "Win+Space",
        "CtrlAltSpace" => "Ctrl+Alt+Space",
        _              => raw
    };

    /// <summary>Parses 'Alt+Space', 'Ctrl+F10', 'Ctrl+Alt+S' etc. into Win32 modifier flags + VK code.</summary>
    public (uint modifiers, uint vk) ParseHotkey()
    {
        uint mods = 0;
        uint vk   = 0;

        var parts = Hotkey.Split('+', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        foreach (var part in parts)
        {
            switch (part.ToUpperInvariant())
            {
                case "ALT":   mods |= HotkeyManager.MOD_ALT;     break;
                case "CTRL":
                case "CONTROL": mods |= HotkeyManager.MOD_CONTROL; break;
                case "SHIFT": mods |= HotkeyManager.MOD_SHIFT;   break;
                case "WIN":
                case "WINDOWS": mods |= HotkeyManager.MOD_WIN;   break;
                default:
                    // Try to map key name to VK code
                    vk = MapKeyName(part);
                    break;
            }
        }

        // Default fallback: Alt+Space
        if (mods == 0) mods = HotkeyManager.MOD_ALT;
        if (vk   == 0) vk   = 0x20; // VK_SPACE

        return (mods, vk);
    }

    private static uint MapKeyName(string key) => key.ToUpperInvariant() switch
    {
        "SPACE"  => 0x20,
        "ENTER"  => 0x0D,
        "TAB"    => 0x09,
        "ESCAPE" or "ESC" => 0x1B,
        "F1"  => 0x70, "F2"  => 0x71, "F3"  => 0x72, "F4"  => 0x73,
        "F5"  => 0x74, "F6"  => 0x75, "F7"  => 0x76, "F8"  => 0x77,
        "F9"  => 0x78, "F10" => 0x79, "F11" => 0x7A, "F12" => 0x7B,
        "A" => 0x41, "B" => 0x42, "C" => 0x43, "D" => 0x44, "E" => 0x45,
        "F" => 0x46, "G" => 0x47, "H" => 0x48, "I" => 0x49, "J" => 0x4A,
        "K" => 0x4B, "L" => 0x4C, "M" => 0x4D, "N" => 0x4E, "O" => 0x4F,
        "P" => 0x50, "Q" => 0x51, "R" => 0x52, "S" => 0x53, "T" => 0x54,
        "U" => 0x55, "V" => 0x56, "W" => 0x57, "X" => 0x58, "Y" => 0x59,
        "Z" => 0x5A,
        "0" => 0x30, "1" => 0x31, "2" => 0x32, "3" => 0x33, "4" => 0x34,
        "5" => 0x35, "6" => 0x36, "7" => 0x37, "8" => 0x38, "9" => 0x39,
        _ => 0
    };
}
