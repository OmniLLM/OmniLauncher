using System.IO;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Input;
using System.Windows.Interop;
using OmniLauncher.Core;
using OmniLauncher.Plugins;
using OmniLauncher.Windows;

namespace OmniLauncher;

public partial class MainWindow : Window, IPublicAPI
{
    private const int HOTKEY_ID = 1;
    private HotkeyManager? _hotkey;
    private readonly AppSettings _cfg;

    public PluginManager PluginManager { get; }

    public MainWindow()
    {
        InitializeComponent();
        _cfg = AppSettings.Load();
        PluginManager = new PluginManager(this);
        LoadAllPlugins();

        Deactivated += (_, _) => { if (IsVisible) Hide(); };
    }

    protected override void OnSourceInitialized(EventArgs e)
    {
        base.OnSourceInitialized(e);
        var helper = new WindowInteropHelper(this);

        var (mod, vk) = _cfg.ParseHotkey();

        try
        {
            _hotkey = new HotkeyManager(helper.Handle, HOTKEY_ID, mod, vk);
            _hotkey.HotkeyPressed += ToggleWindow;
        }
        catch (InvalidOperationException ex)
        {
            System.Diagnostics.Debug.WriteLine($"Hotkey registration failed: {ex.Message}");
        }

        HwndSource.FromHwnd(helper.Handle)!.AddHook(WndProc);
    }

    private IntPtr WndProc(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
        const int WM_HOTKEY = 0x0312;
        if (msg == WM_HOTKEY) { _hotkey?.ProcessMessage(wParam.ToInt32()); handled = true; }
        return IntPtr.Zero;
    }

    public void ToggleWindow()
    {
        if (IsVisible) { Hide(); }
        else
        {
            SearchBox.Clear();
            ResultsList.Items.Clear();
            Show();
            Activate();
            SearchBox.Focus();
        }
    }

    private void LoadAllPlugins()
    {
        var ctx = new PluginInitContext(AppDomain.CurrentDomain.BaseDirectory, this);
        PluginManager.RegisterPlugin(new AppLauncherPlugin(),     ctx);
        PluginManager.RegisterPlugin(new WebSearchPlugin(),     ctx);
        PluginManager.RegisterPlugin(new CalculatorPlugin(),    ctx);
        PluginManager.RegisterPlugin(new FileSearchPlugin(),    ctx);
        PluginManager.RegisterPlugin(new ClipboardPlugin(),     ctx);
        PluginManager.RegisterPlugin(new OmniLLMPlugin(),       ctx);
        PluginManager.RegisterPlugin(new ShellPlugin(),         ctx);
        PluginManager.RegisterPlugin(new WindowWalkerPlugin(),  ctx);
        PluginManager.RegisterPlugin(new SystemCommandsPlugin(),ctx);
        PluginManager.RegisterPlugin(new ProcessKillerPlugin(), ctx);
        PluginManager.RegisterPlugin(new ColorPlugin(),         ctx);
        PluginManager.RegisterPlugin(new UnitConverterPlugin(), ctx);
        PluginManager.RegisterPlugin(new TimerPlugin(),         ctx);

        // Load external plugins from Plugins/ directory
        var pluginDir = System.IO.Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "Plugins");
        PluginManager.LoadPluginsFromDirectory(pluginDir);
    }

    private void SearchBox_TextChanged(object sender, System.Windows.Controls.TextChangedEventArgs e)
    {
        Placeholder.Visibility = string.IsNullOrEmpty(SearchBox.Text) ? Visibility.Visible : Visibility.Collapsed;
        RefreshResults(SearchBox.Text);
    }

    private void RefreshResults(string query)
    {
        ResultsList.Items.Clear();
        if (string.IsNullOrWhiteSpace(query)) return;

        foreach (var r in PluginManager.QueryAll(query).Take(_cfg.MaxResults))
            ResultsList.Items.Add(r);

        if (ResultsList.Items.Count > 0)
            ResultsList.SelectedIndex = 0;
    }

    private void SearchBox_KeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Escape) { Hide(); e.Handled = true; }
        if (e.Key == Key.Enter) { ExecuteSelected(); e.Handled = true; }
        // Ctrl+, opens settings
        if (e.Key == Key.OemComma && Keyboard.Modifiers == ModifierKeys.Control)
        {
            new SettingsWindow().Show();
            e.Handled = true;
        }
    }

    private void SearchBox_PreviewKeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Down && ResultsList.Items.Count > 0)
        {
            ResultsList.SelectedIndex = Math.Min(ResultsList.SelectedIndex + 1, ResultsList.Items.Count - 1);
            e.Handled = true;
        }
        if (e.Key == Key.Up && ResultsList.Items.Count > 0)
        {
            ResultsList.SelectedIndex = Math.Max(ResultsList.SelectedIndex - 1, 0);
            e.Handled = true;
        }
    }

    private void ResultsList_MouseDoubleClick(object sender, MouseButtonEventArgs e) => ExecuteSelected();
    private void ResultsList_KeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter) ExecuteSelected();
    }

    private void ExecuteSelected()
    {
        if (ResultsList.SelectedItem is Result r)
        {
            var ctx = new ActionContext(Keyboard.IsKeyDown(Key.LeftCtrl));
            if (r.Action?.Invoke(ctx) == true) Hide();
        }
    }

    protected override void OnClosed(EventArgs e) { _hotkey?.Dispose(); base.OnClosed(e); }

    public void ChangeQuery(string query, bool requery = false)
    {
        SearchBox.Text = query;
        SearchBox.CaretIndex = query.Length;
        if (requery) RefreshResults(query);
    }
    public void HideMainWindow() => Hide();
    public void ShowMainWindow() { Show(); Activate(); SearchBox.Focus(); }
}
