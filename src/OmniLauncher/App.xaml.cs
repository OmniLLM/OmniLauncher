using System.Windows;
using OmniLauncher.Core;

namespace OmniLauncher;

public partial class App : Application
{
    public static PluginManager PluginManager { get; private set; } = null!;
    private TrayIcon? _tray;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        var mainWindow = new MainWindow();
        PluginManager = mainWindow.PluginManager;
        _tray = new TrayIcon(mainWindow);

        // Read settings
        var cfg = AppSettings.Load();
        if (!cfg.HideOnLaunch)
            mainWindow.Show();

        // Keep app alive without a main window
        ShutdownMode = ShutdownMode.OnExplicitShutdown;
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _tray?.Dispose();
        base.OnExit(e);
    }
}