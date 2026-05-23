using OmniLauncher.Core;
using System.Windows;

namespace OmniLauncher.Plugins;

/// <summary>
/// Convert units. Prefix: conv — e.g. conv 5 km to miles, conv 100 f to c
/// </summary>
public class UnitConverterPlugin : IPlugin
{
    public string  Name        => "Unit Converter";
    public string  Description => "Convert units. Prefix: conv — e.g. conv 5 km to miles";
    public string? Keyword     => "conv ";

    public void Init(PluginInitContext context) { }

    public IList<Result> Query(Query query)
    {
        if (!TryConvert(query.Search.Trim(), out var result, out var display))
            return Array.Empty<Result>();

        return new[]
        {
            new Result
            {
                Title    = display,
                SubTitle = "Click to copy",
                Score    = 100,
                Action   = _ => { Clipboard.SetText(result); return true; }
            }
        };
    }

    private static bool TryConvert(string input, out string result, out string display)
    {
        result = display = "";
        // Pattern: <number> <from_unit> [to] <to_unit>
        var m = System.Text.RegularExpressions.Regex.Match(input,
            @"^([\d.]+)\s+(\w+)\s+(?:to\s+)?(\w+)$", System.Text.RegularExpressions.RegexOptions.IgnoreCase);
        if (!m.Success) return false;

        if (!double.TryParse(m.Groups[1].Value, out double val)) return false;
        var from = m.Groups[2].Value.ToLowerInvariant();
        var to   = m.Groups[3].Value.ToLowerInvariant();

        double? converted = (from, to) switch
        {
            // Length
            ("km",  "miles") or ("km",  "mi")  => val * 0.621371,
            ("miles","km")   or ("mi",  "km")  => val * 1.60934,
            ("m",   "ft")   or ("m",   "feet") => val * 3.28084,
            ("ft",  "m")    or ("feet","m")    => val * 0.3048,
            ("cm",  "in")   or ("cm",  "inches")=> val * 0.393701,
            ("in",  "cm")   or ("inches","cm") => val * 2.54,
            // Weight
            ("kg",  "lbs")  or ("kg",  "lb")  => val * 2.20462,
            ("lbs", "kg")   or ("lb",  "kg")  => val * 0.453592,
            ("g",   "oz")                      => val * 0.035274,
            ("oz",  "g")                       => val * 28.3495,
            // Temperature
            ("c",   "f")    or ("celsius","fahrenheit") => val * 9 / 5 + 32,
            ("f",   "c")    or ("fahrenheit","celsius") => (val - 32) * 5 / 9,
            ("c",   "k")    or ("celsius","kelvin")     => val + 273.15,
            ("k",   "c")    or ("kelvin","celsius")     => val - 273.15,
            // Speed
            ("kmh", "mph")  or ("kph", "mph")  => val * 0.621371,
            ("mph", "kmh")  or ("mph", "kph")  => val * 1.60934,
            ("ms",  "mph")                     => val * 2.23694,
            // Data
            ("mb",  "gb")                      => val / 1024,
            ("gb",  "mb")                      => val * 1024,
            ("gb",  "tb")                      => val / 1024,
            ("tb",  "gb")                      => val * 1024,
            _ => null
        };

        if (converted is null) return false;
        result  = $"{converted.Value:G6}";
        display = $"{val} {from} = {result} {to}";
        return true;
    }
}
