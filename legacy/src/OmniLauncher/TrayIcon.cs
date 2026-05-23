using System.Drawing;
using System.Windows.Forms;
using OmniLauncher.Windows;

namespace OmniLauncher;

/// <summary>System tray icon with context menu.</summary>
public sealed class TrayIcon : IDisposable
{
    private readonly NotifyIcon _icon;
    private readonly MainWindow _mainWindow;

    public TrayIcon(MainWindow mainWindow)
    {
        _mainWindow = mainWindow;

        _icon = new NotifyIcon
        {
            Text = "OmniLauncher",
            Icon = SystemIcons.Application,
            Visible = true,
            ContextMenuStrip = BuildMenu()
        };

        _icon.DoubleClick += (_, _) => _mainWindow.ShowMainWindow();
    }

    private ContextMenuStrip BuildMenu()
    {
        var menu = new ContextMenuStrip();
        menu.Items.Add("Show (Alt+Space)", null, (_, _) => _mainWindow.ShowMainWindow());
        menu.Items.Add("Settings",         null, (_, _) => new SettingsWindow().Show());
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add("Exit",             null, (_, _) => System.Windows.Application.Current.Shutdown());
        return menu;
    }

    public void Dispose() { _icon.Visible = false; _icon.Dispose(); }
}