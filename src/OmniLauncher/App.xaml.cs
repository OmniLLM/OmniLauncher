using System.Windows;
using OmniLauncher.Core;

namespace OmniLauncher;

public partial class App : Application
{
    public static PluginManager PluginManager { get; private set; } = null!;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        var mainWindow = new MainWindow();
        PluginManager = mainWindow.PluginManager;
        mainWindow.Show();
    }
}