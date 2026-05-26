/// Window resize / tile plugin — ported from Raycast `window-layouts` extension concept.
/// Uses platform-native tools:
///   Linux  — wmctrl (must be installed: apt install wmctrl)
///   macOS  — AppleScript / System Events (requires Accessibility permission)
///   Windows — PowerShell with user32.dll / Set-WindowPosition  
///
/// Prefix: "resize "
/// Examples:
///   resize fullscreen
///   resize left half
///   resize right half
///   resize top half
///   resize bottom half
///   resize top left
///   resize top right
///   resize bottom left
///   resize bottom right
///   resize center
///   resize maximize
///   resize restore
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct WindowResizePlugin;

#[derive(Clone)]
struct Layout {
    keyword: &'static str,
    label: &'static str,
    icon: &'static str,
    /// Fractions: (x, y, w, h) relative to screen (0.0..1.0)
    rect: (f64, f64, f64, f64),
}

static LAYOUTS: &[Layout] = &[
    Layout {
        keyword: "fullscreen",
        label: "Fullscreen",
        icon: "⛶",
        rect: (0.0, 0.0, 1.0, 1.0),
    },
    Layout {
        keyword: "maximize",
        label: "Maximize",
        icon: "🔲",
        rect: (0.0, 0.0, 1.0, 1.0),
    },
    Layout {
        keyword: "left half",
        label: "Left Half",
        icon: "◧",
        rect: (0.0, 0.0, 0.5, 1.0),
    },
    Layout {
        keyword: "right half",
        label: "Right Half",
        icon: "◨",
        rect: (0.5, 0.0, 0.5, 1.0),
    },
    Layout {
        keyword: "top half",
        label: "Top Half",
        icon: "⬒",
        rect: (0.0, 0.0, 1.0, 0.5),
    },
    Layout {
        keyword: "bottom half",
        label: "Bottom Half",
        icon: "⬓",
        rect: (0.0, 0.5, 1.0, 0.5),
    },
    Layout {
        keyword: "top left",
        label: "Top Left Quarter",
        icon: "◸",
        rect: (0.0, 0.0, 0.5, 0.5),
    },
    Layout {
        keyword: "top right",
        label: "Top Right Quarter",
        icon: "◹",
        rect: (0.5, 0.0, 0.5, 0.5),
    },
    Layout {
        keyword: "bottom left",
        label: "Bottom Left Quarter",
        icon: "◺",
        rect: (0.0, 0.5, 0.5, 0.5),
    },
    Layout {
        keyword: "bottom right",
        label: "Bottom Right Quarter",
        icon: "◻",
        rect: (0.5, 0.5, 0.5, 0.5),
    },
    Layout {
        keyword: "center",
        label: "Center (80%)",
        icon: "⊡",
        rect: (0.1, 0.05, 0.8, 0.9),
    },
    Layout {
        keyword: "wide center",
        label: "Wide Center",
        icon: "▬",
        rect: (0.0, 0.1, 1.0, 0.8),
    },
    Layout {
        keyword: "left 70",
        label: "Left 70%",
        icon: "▏",
        rect: (0.0, 0.0, 0.7, 1.0),
    },
    Layout {
        keyword: "right 70",
        label: "Right 70%",
        icon: "▕",
        rect: (0.3, 0.0, 0.7, 1.0),
    },
    Layout {
        keyword: "left 30",
        label: "Left 30%",
        icon: "▎",
        rect: (0.0, 0.0, 0.3, 1.0),
    },
    Layout {
        keyword: "right 30",
        label: "Right 30%",
        icon: "▊",
        rect: (0.7, 0.0, 0.3, 1.0),
    },
];

/// Build the shell command to move the active window to the given rect on the screen.
fn build_resize_command(rect: (f64, f64, f64, f64)) -> String {
    let (rx, ry, rw, rh) = rect;

    #[cfg(target_os = "linux")]
    {
        // wmctrl: first remove maximized state, then set geometry
        // We need actual screen size — use xdpyinfo or fallback to 1920x1080
        // The script computes screen dimensions at runtime
        format!(
            r#"sh -c '
SW=$(xdpyinfo 2>/dev/null | awk "/dimensions:/ {{print \$2}}" | cut -dx -f1)
SH=$(xdpyinfo 2>/dev/null | awk "/dimensions:/ {{print \$2}}" | cut -dx -f2)
SW=${{SW:-1920}}; SH=${{SH:-1080}}
X=$(python3 -c "print(int({rx}*$SW))")
Y=$(python3 -c "print(int({ry}*$SH))")
W=$(python3 -c "print(int({rw}*$SW))")
H=$(python3 -c "print(int({rh}*$SH))")
WID=$(xdotool getactivewindow 2>/dev/null)
wmctrl -ir "$WID" -b remove,maximized_vert,maximized_horz 2>/dev/null
wmctrl -ir "$WID" -e "0,$X,$Y,$W,$H" 2>/dev/null || \
  wmctrl -r ":ACTIVE:" -b remove,maximized_vert,maximized_horz && wmctrl -r ":ACTIVE:" -e "0,$X,$Y,$W,$H"
'"#
        )
    }

    #[cfg(target_os = "macos")]
    {
        // AppleScript: get screen size, then set frontmost window bounds
        format!(
            r#"osascript -e '
set sw to do shell script "system_profiler SPDisplaysDataType | awk \"/Resolution:/ {{print $2}}\" | head -1"
set sh to do shell script "system_profiler SPDisplaysDataType | awk \"/Resolution:/ {{print $4}}\" | head -1"
set sw to sw as integer
set sh to sh as integer
set x1 to round ({rx} * sw)
set y1 to round ({ry} * sh)
set x2 to round (({rx} + {rw}) * sw)
set y2 to round (({ry} + {rh}) * sh)
tell application "System Events"
  set frontApp to first application process whose frontmost is true
  set frontWindow to first window of frontApp
  set position of frontWindow to {{x1, y1}}
  set size of frontWindow to {{x2 - x1, y2 - y1}}
end tell'"#
        )
    }

    #[cfg(target_os = "windows")]
    {
        // PowerShell: use WinAPI via Add-Type to resize the foreground window
        format!(
            r#"powershell -Command "
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class WinPos {{
    [DllImport(\"user32.dll\")] public static extern IntPtr GetForegroundWindow();
    [DllImport(\"user32.dll\")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int h2, bool r);
    [DllImport(\"user32.dll\")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport(\"user32.dll\")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [StructLayout(LayoutKind.Sequential)] public struct RECT {{ public int L, T, R, B; }}
}}
'@
$sm = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$sw = $sm.Width; $sh = $sm.Height
$x  = [int]({rx} * $sw)
$y  = [int]({ry} * $sh)
$w  = [int]({rw} * $sw)
$h  = [int]({rh} * $sh)
$hwnd = [WinPos]::GetForegroundWindow()
[WinPos]::ShowWindow($hwnd, 1) | Out-Null
[WinPos]::MoveWindow($hwnd, $x, $y, $w, $h, `$true) | Out-Null
""#
        )
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        format!("echo 'Window resize not supported on this platform (rect: {rx},{ry},{rw},{rh})'")
    }
}

#[async_trait]
impl Plugin for WindowResizePlugin {
    fn name(&self) -> &str {
        "window_resize"
    }

    fn description(&self) -> &str {
        "Tile/resize active window — resize left half / right half / fullscreen / top left …"
    }

    fn keyword(&self) -> Option<&str> {
        Some("resize ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let search = q
            .raw
            .strip_prefix("resize ")
            .unwrap_or("")
            .trim()
            .to_lowercase();

        let mut results: Vec<QueryResult> = LAYOUTS
            .iter()
            .filter(|l| {
                search.is_empty()
                    || l.keyword.starts_with(search.as_str())
                    || l.keyword.contains(search.as_str())
                    || l.label.to_lowercase().contains(search.as_str())
            })
            .map(|l| {
                let score = if l.keyword.starts_with(search.as_str()) {
                    90
                } else {
                    70
                };
                QueryResult {
                    id: format!("resize:{}", l.keyword),
                    title: format!("{} {}", l.icon, l.label),
                    subtitle: Some(format!(
                        "Resize active window — {:.0}%×{:.0}% at ({:.0}%, {:.0}%)",
                        l.rect.2 * 100.0,
                        l.rect.3 * 100.0,
                        l.rect.0 * 100.0,
                        l.rect.1 * 100.0,
                    )),
                    icon: Some(l.icon.to_string()),
                    score,
                    action_type: "shell".to_string(),
                    action_data: build_resize_command(l.rect),
                }
            })
            .collect();

        results.sort_by_key(|r| std::cmp::Reverse(r.score));
        results.truncate(10);
        results
    }
}
