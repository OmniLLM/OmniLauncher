using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Input;
using System.Windows.Interop;
using OmniLauncher.Core;

namespace OmniLauncher;

public partial class MainWindow : Window, IPublicAPI
{
    private const int HOTKEY_ID = 1;
    private const uint VK_SPACE = 0x20;
    private HotkeyManager? _hotkey;

    public PluginManager PluginManager { get; }

    public MainWindow()
    {
        InitializeComponent();
        PluginManager = new PluginManager(this);

        // Load built-in + external plugins
        LoadBuiltinPlugins();
        var pluginDir = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "Plugins");
        PluginManager.LoadPluginsFromDirectory(pluginDir);

        Deactivated += (_, _) => Hide();
    }

    protected override void OnSourceInitialized(EventArgs e)
    {
        base.OnSourceInitialized(e);
        var helper = new WindowInteropHelper(this);
        _hotkey = new HotkeyManager(helper.Handle, HOTKEY_ID, HotkeyManager.MOD_ALT, VK_SPACE);
        _hotkey.HotkeyPressed += ToggleWindow;

        HwndSource.FromHwnd(helper.Handle)!.AddHook(WndProc);
    }

    private IntPtr WndProc(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
        const int WM_HOTKEY = 0x0312;
        if (msg == WM_HOTKEY) { _hotkey?.ProcessMessage(wParam.ToInt32()); handled = true; }
        return IntPtr.Zero;
    }

    private void ToggleWindow()
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

    private void LoadBuiltinPlugins()
    {
        var ctx = new PluginInitContext(AppDomain.CurrentDomain.BaseDirectory, this);
        PluginManager.RegisterPlugin(new OmniLauncher.Plugins.AppLauncherPlugin(), ctx);
        PluginManager.RegisterPlugin(new OmniLauncher.Plugins.WebSearchPlugin(), ctx);
        PluginManager.RegisterPlugin(new OmniLauncher.Plugins.CalculatorPlugin(), ctx);
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

        foreach (var r in PluginManager.QueryAll(query).Take(8))
            ResultsList.Items.Add(r);

        if (ResultsList.Items.Count > 0)
            ResultsList.SelectedIndex = 0;
    }

    private void SearchBox_KeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Escape) { Hide(); e.Handled = true; }
        if (e.Key == Key.Enter) { ExecuteSelected(); e.Handled = true; }
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

    // IPublicAPI
    public void ChangeQuery(string query, bool requery = false)
    {
        SearchBox.Text = query;
        SearchBox.CaretIndex = query.Length;
        if (requery) RefreshResults(query);
    }
    public void HideMainWindow() => Hide();
    public void ShowMainWindow() { Show(); Activate(); SearchBox.Focus(); }
}