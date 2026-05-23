using System.IO;
using System.Text.Json;
using System.Windows;
using Microsoft.Win32;

namespace OmniLauncher.Windows;

public partial class SettingsWindow : Window
{
    private static readonly string SettingsPath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "OmniLauncher", "settings.json");

    public SettingsWindow()
    {
        InitializeComponent();
        LoadSettings();
    }

    private void LoadSettings()
    {
        if (!File.Exists(SettingsPath)) return;
        try
        {
            var cfg = JsonDocument.Parse(File.ReadAllText(SettingsPath)).RootElement;
            if (cfg.TryGetProperty("hotkey", out var hk))
                SelectComboByTag(HotkeyCombo, hk.GetString());
            if (cfg.TryGetProperty("theme", out var th))
                SelectComboByTag(ThemeCombo, th.GetString());
            if (cfg.TryGetProperty("omniLLMUrl", out var u))
                OmniLLMUrl.Text = u.GetString() ?? OmniLLMUrl.Text;
            if (cfg.TryGetProperty("omniLLMModel", out var m))
                OmniLLMModel.Text = m.GetString() ?? OmniLLMModel.Text;
            if (cfg.TryGetProperty("maxResults", out var mr))
                MaxResultsSlider.Value = mr.GetDouble();
            if (cfg.TryGetProperty("startOnBoot", out var sb))
                StartOnBoot.IsChecked = sb.GetBoolean();
            if (cfg.TryGetProperty("hideOnLaunch", out var hl))
                HideOnLaunch.IsChecked = hl.GetBoolean();
        }
        catch { }
    }

    private void SaveBtn_Click(object sender, RoutedEventArgs e)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(SettingsPath)!);

        var settings = new
        {
            hotkey      = GetSelectedTag(HotkeyCombo),
            theme       = GetSelectedTag(ThemeCombo),
            omniLLMUrl  = OmniLLMUrl.Text.Trim(),
            omniLLMModel = OmniLLMModel.Text.Trim(),
            maxResults  = (int)MaxResultsSlider.Value,
            startOnBoot = StartOnBoot.IsChecked == true,
            hideOnLaunch = HideOnLaunch.IsChecked == true,
        };

        File.WriteAllText(SettingsPath, JsonSerializer.Serialize(settings, new JsonSerializerOptions { WriteIndented = true }));

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