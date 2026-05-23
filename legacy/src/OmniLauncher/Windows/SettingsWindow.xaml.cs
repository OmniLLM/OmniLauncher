using System.IO;
using System.Text.Json;
using System.Windows;
using System.Windows.Input;
using Microsoft.Win32;

namespace OmniLauncher.Windows;

public partial class SettingsWindow : Window
{
    private static readonly string SettingsPath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "OmniLauncher", "settings.json");

    // Hotkey recording state
    private bool   _recording    = false;
    private string _savedHotkey  = "Alt+Space";
    private string _pendingHotkey = "";

    // Provider base-URL presets
    private static readonly Dictionary<string, string> ProviderUrls = new()
    {
        ["omnillm"] = "http://localhost:5000",
        ["openai"]  = "https://api.openai.com",
        ["azure"]   = "https://YOUR_RESOURCE.openai.azure.com",
        ["custom"]  = "",
    };

    public SettingsWindow()
    {
        InitializeComponent();
        LoadSettings();
    }

    // ── Load ────────────────────────────────────────────────────────────────

    private void LoadSettings()
    {
        if (!File.Exists(SettingsPath)) return;
        try
        {
            var cfg = JsonDocument.Parse(File.ReadAllText(SettingsPath)).RootElement;

            if (cfg.TryGetProperty("hotkey", out var hk))
            {
                _savedHotkey     = NormaliseHotkey(hk.GetString() ?? "Alt+Space");
                HotkeyBox.Text   = _savedHotkey;
            }

            if (cfg.TryGetProperty("theme", out var th))
                SelectComboByTag(ThemeCombo, th.GetString());

            if (cfg.TryGetProperty("provider", out var prov))
                SelectComboByTag(ProviderCombo, prov.GetString());

            if (cfg.TryGetProperty("omniLLMUrl", out var u))
                OmniLLMUrl.Text = u.GetString() ?? OmniLLMUrl.Text;

            if (cfg.TryGetProperty("omniLLMApiKey", out var k) && !string.IsNullOrEmpty(k.GetString()))
                ApiKeyBox.Password = k.GetString();

            if (cfg.TryGetProperty("omniLLMModel", out var m))
                OmniLLMModel.Text = m.GetString() ?? OmniLLMModel.Text;

            if (cfg.TryGetProperty("omniLLMMaxTokens", out var mt))
                MaxTokensSlider.Value = mt.GetDouble();

            if (cfg.TryGetProperty("maxResults", out var mr))
                MaxResultsSlider.Value = mr.GetDouble();

            if (cfg.TryGetProperty("startOnBoot", out var sb))
                StartOnBoot.IsChecked = sb.GetBoolean();

            if (cfg.TryGetProperty("hideOnLaunch", out var hl))
                HideOnLaunch.IsChecked = hl.GetBoolean();
        }
        catch { }
    }

    private static string NormaliseHotkey(string raw) => raw switch
    {
        "AltSpace"     => "Alt+Space",
        "CtrlSpace"    => "Ctrl+Space",
        "WinSpace"     => "Win+Space",
        "CtrlAltSpace" => "Ctrl+Alt+Space",
        _              => raw
    };

    // ── Hotkey Recording ────────────────────────────────────────────────────

    private void RecordBtn_Click(object sender, RoutedEventArgs e)
    {
        if (_recording) StopRecording(cancel: true);
        else            StartRecording();
    }

    private void StartRecording()
    {
        _recording     = true;
        _savedHotkey   = HotkeyBox.Text;
        _pendingHotkey = "";
        RecordBtn.Content     = "Cancel";
        RecordBtn.Background  = new System.Windows.Media.SolidColorBrush(
            System.Windows.Media.Color.FromRgb(0xF3, 0x8B, 0xA8)); // red
        HotkeyBox.Text        = "Press a modifier + key…";
        HotkeyHint.Text       = "Hold a modifier (Alt/Ctrl/Shift/Win) then press a key. Escape to cancel.";
        HotkeyBox.Focus();
    }

    private void StopRecording(bool cancel)
    {
        _recording = false;
        RecordBtn.Content    = "Record";
        RecordBtn.Background = new System.Windows.Media.SolidColorBrush(
            System.Windows.Media.Color.FromRgb(0x31, 0x32, 0x44)); // default
        HotkeyHint.Text = "Click 'Record' then press a modifier+key combination";

        if (cancel || string.IsNullOrEmpty(_pendingHotkey))
            HotkeyBox.Text = _savedHotkey;
        else
            HotkeyBox.Text = _pendingHotkey;
    }

    private void HotkeyBox_GotFocus(object sender, RoutedEventArgs e)
    {
        if (!_recording) StartRecording();
    }

    private void HotkeyBox_LostFocus(object sender, RoutedEventArgs e)
    {
        if (_recording) StopRecording(cancel: true);
    }

    private void HotkeyBox_PreviewKeyDown(object sender, KeyEventArgs e)
    {
        if (!_recording) return;
        e.Handled = true;

        if (e.Key == Key.Escape) { StopRecording(cancel: true); return; }

        // Build modifier string
        var mods = new List<string>();
        if ((Keyboard.Modifiers & ModifierKeys.Control) != 0) mods.Add("Ctrl");
        if ((Keyboard.Modifiers & ModifierKeys.Alt)     != 0) mods.Add("Alt");
        if ((Keyboard.Modifiers & ModifierKeys.Shift)   != 0) mods.Add("Shift");
        if ((Keyboard.Modifiers & ModifierKeys.Windows) != 0) mods.Add("Win");

        // Ignore if only modifier keys pressed (no actual key yet)
        var key = e.Key == Key.System ? e.SystemKey : e.Key;
        if (key is Key.LeftCtrl or Key.RightCtrl or Key.LeftAlt or Key.RightAlt
                or Key.LeftShift or Key.RightShift or Key.LWin or Key.RWin)
        {
            // Show partial combo as user holds modifiers
            HotkeyBox.Text = mods.Count > 0 ? string.Join("+", mods) + "+…" : "Press a modifier + key…";
            return;
        }

        if (mods.Count == 0)
        {
            HotkeyBox.Text = "⚠ Requires a modifier key (Alt, Ctrl, Shift, Win)";
            _pendingHotkey = "";
            return;
        }

        // Map WPF Key to friendly name
        string keyName = key switch
        {
            Key.Space  => "Space",
            Key.Return => "Enter",
            Key.Tab    => "Tab",
            >= Key.F1 and <= Key.F12 => key.ToString(), // F1..F12
            _ => key.ToString() // A-Z, 0-9, etc.
        };

        _pendingHotkey = string.Join("+", mods) + "+" + keyName;
        HotkeyBox.Text = _pendingHotkey;
        StopRecording(cancel: false);
    }



    // ── Provider preset URLs ─────────────────────────────────────────────────

    private void ProviderCombo_SelectionChanged(object sender, System.Windows.Controls.SelectionChangedEventArgs e)
    {
        if (ProviderCombo.SelectedItem is not System.Windows.Controls.ComboBoxItem item) return;
        var tag = item.Tag?.ToString() ?? "omnillm";
        if (ProviderUrls.TryGetValue(tag, out var url) && !string.IsNullOrEmpty(url))
            OmniLLMUrl.Text = url;
    }

    // ── Save ─────────────────────────────────────────────────────────────────

    private void SaveBtn_Click(object sender, RoutedEventArgs e)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(SettingsPath)!);

        var settings = new
        {
            hotkey           = HotkeyBox.Text,
            theme            = GetSelectedTag(ThemeCombo),
            provider         = GetSelectedTag(ProviderCombo),
            omniLLMUrl       = OmniLLMUrl.Text.Trim(),
            omniLLMApiKey    = ApiKeyBox.Password,
            omniLLMModel     = OmniLLMModel.Text.Trim(),
            omniLLMMaxTokens = (int)MaxTokensSlider.Value,
            maxResults       = (int)MaxResultsSlider.Value,
            startOnBoot      = StartOnBoot.IsChecked == true,
            hideOnLaunch     = HideOnLaunch.IsChecked == true,
        };

        File.WriteAllText(SettingsPath,
            JsonSerializer.Serialize(settings, new JsonSerializerOptions { WriteIndented = true }));

        // Startup registry
        using var key = Registry.CurrentUser.OpenSubKey(@"SOFTWARE\Microsoft\Windows\CurrentVersion\Run", true)!;
        if (settings.startOnBoot)
            key.SetValue("OmniLauncher", $"\"{Environment.ProcessPath}\"");
        else
            key.DeleteValue("OmniLauncher", false);

        MessageBox.Show("Settings saved. Restart OmniLauncher to apply hotkey/theme changes.",
            "OmniLauncher", MessageBoxButton.OK, MessageBoxImage.Information);
        Close();
    }

    private void CancelBtn_Click(object sender, RoutedEventArgs e) => Close();

    private static void SelectComboByTag(System.Windows.Controls.ComboBox cb, string? tag)
    {
        foreach (System.Windows.Controls.ComboBoxItem item in cb.Items)
            if (item.Tag?.ToString() == tag) { cb.SelectedItem = item; return; }
    }

    private static string? GetSelectedTag(System.Windows.Controls.ComboBox cb)
        => (cb.SelectedItem as System.Windows.Controls.ComboBoxItem)?.Tag?.ToString();
}
