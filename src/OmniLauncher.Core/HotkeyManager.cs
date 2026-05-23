using System.Runtime.InteropServices;

namespace OmniLauncher.Core;

/// <summary>
/// Registers a global hotkey using Win32 RegisterHotKey.
/// Default: Alt+Space (id=1).
/// </summary>
public sealed class HotkeyManager : IDisposable
{
    [DllImport("user32.dll")] private static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);
    [DllImport("user32.dll")] private static extern bool UnregisterHotKey(IntPtr hWnd, int id);

    // Modifier flags
    public const uint MOD_ALT     = 0x0001;
    public const uint MOD_CONTROL = 0x0002;
    public const uint MOD_SHIFT   = 0x0004;
    public const uint MOD_WIN     = 0x0008;
    public const uint MOD_NOREPEAT = 0x4000;

    private readonly IntPtr _hwnd;
    private readonly int _id;

    public event Action? HotkeyPressed;

    public HotkeyManager(IntPtr hwnd, int id, uint modifiers, uint vk)
    {
        _hwnd = hwnd;
        _id = id;
        if (!RegisterHotKey(hwnd, id, modifiers | MOD_NOREPEAT, vk))
            throw new InvalidOperationException($"Failed to register hotkey (id={id}). It may already be in use.");
    }

    /// <summary>Call this from your WndProc when WM_HOTKEY arrives.</summary>
    public void ProcessMessage(int id)
    {
        if (id == _id) HotkeyPressed?.Invoke();
    }

    public void Dispose() => UnregisterHotKey(_hwnd, _id);
}