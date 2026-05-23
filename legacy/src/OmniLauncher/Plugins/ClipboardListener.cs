using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;

namespace OmniLauncher.Plugins;

/// <summary>
/// Listens for clipboard changes via Win32 AddClipboardFormatListener.
/// Must be created on the UI thread.
/// </summary>
internal sealed class ClipboardListener : IDisposable
{
    [DllImport("user32.dll")] private static extern bool AddClipboardFormatListener(IntPtr hwnd);
    [DllImport("user32.dll")] private static extern bool RemoveClipboardFormatListener(IntPtr hwnd);

    private const int WM_CLIPBOARDUPDATE = 0x031D;

    public event Action<string>? ClipboardChanged;

    private readonly HwndSource _source;

    public ClipboardListener()
    {
        var helper = new HwndSource(new HwndSourceParameters("ClipboardWatcher") { Width = 0, Height = 0 });
        helper.AddHook(WndProc);
        AddClipboardFormatListener(helper.Handle);
        _source = helper;
    }

    private IntPtr WndProc(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
        if (msg == WM_CLIPBOARDUPDATE)
        {
            try
            {
                if (Clipboard.ContainsText())
                    ClipboardChanged?.Invoke(Clipboard.GetText());
            }
            catch { /* clipboard race */ }
        }
        return IntPtr.Zero;
    }

    public void Dispose()
    {
        RemoveClipboardFormatListener(_source.Handle);
        _source.Dispose();
    }
}