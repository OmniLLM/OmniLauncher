using System.Windows;
using OmniLauncher.Core;

namespace OmniLauncher.Plugins;

/// <summary>
/// Convert color formats. Prefix: # — e.g. #1e1e2e, #fff, rgb(30,30,46)
/// </summary>
public class ColorPlugin : IPlugin
{
    public string  Name        => "Color";
    public string  Description => "Convert color formats. Prefix: #";
    public string? Keyword     => "#";

    public void Init(PluginInitContext context) { }

    public IList<Result> Query(Query query)
    {
        var input = ("#" + query.Search).Trim();
        if (!TryParse(input, out byte r, out byte g, out byte b)) return Array.Empty<Result>();

        var hex  = $"#{r:X2}{g:X2}{b:X2}";
        var rgb  = $"rgb({r}, {g}, {b})";
        var hsl  = ToHsl(r, g, b);
        var entries = new[] { hex, rgb, hsl };

        return entries.Select(val => new Result
        {
            Title    = val,
            SubTitle = $"Click to copy · preview: {hex}",
            Score    = 95,
            Action   = _ => { Clipboard.SetText(val); return true; }
        }).ToList();
    }

    private static bool TryParse(string s, out byte r, out byte g, out byte b)
    {
        r = g = b = 0;
        s = s.Trim();

        // #RGB or #RRGGBB
        if (s.StartsWith('#'))
        {
            s = s[1..];
            if (s.Length == 3) s = $"{s[0]}{s[0]}{s[1]}{s[1]}{s[2]}{s[2]}";
            if (s.Length == 6 && int.TryParse(s, System.Globalization.NumberStyles.HexNumber, null, out var hex))
            {
                r = (byte)((hex >> 16) & 0xFF);
                g = (byte)((hex >>  8) & 0xFF);
                b = (byte)( hex        & 0xFF);
                return true;
            }
        }

        // rgb(r, g, b)
        var m = System.Text.RegularExpressions.Regex.Match(s, @"rgb\s*\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)");
        if (m.Success)
        {
            r = byte.Parse(m.Groups[1].Value);
            g = byte.Parse(m.Groups[2].Value);
            b = byte.Parse(m.Groups[3].Value);
            return true;
        }
        return false;
    }

    private static string ToHsl(byte r, byte g, byte b)
    {
        double rf = r / 255.0, gf = g / 255.0, bf = b / 255.0;
        double max = Math.Max(rf, Math.Max(gf, bf));
        double min = Math.Min(rf, Math.Min(gf, bf));
        double l   = (max + min) / 2;
        if (max == min) return $"hsl(0, 0%, {(int)(l * 100)}%)";
        double d = max - min;
        double s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
        double h = max == rf ? (gf - bf) / d + (gf < bf ? 6 : 0)
                 : max == gf ? (bf - rf) / d + 2
                 :             (rf - gf) / d + 4;
        return $"hsl({(int)(h / 6 * 360)}, {(int)(s * 100)}%, {(int)(l * 100)}%)";
    }
}
