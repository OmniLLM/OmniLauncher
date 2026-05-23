using System.Data;
using OmniLauncher.Windows;
using OmniLauncher.Core;

namespace OmniLauncher.Plugins;

/// <summary>Evaluates simple math expressions. Prefix: = </summary>
public class CalculatorPlugin : IPlugin
{
    public string Name => "Calculator";
    public string Description => "Evaluate math. Prefix: =";
    public string? Keyword => "=";

    public void Init(PluginInitContext context) { }

    public IList<Result> Query(Query query)
    {
        var expr = query.Search.Trim();
        if (string.IsNullOrWhiteSpace(expr)) return Array.Empty<Result>();

        try
        {
            var result = new DataTable().Compute(expr, null);
            var display = $"= {result}";
            return new[]
            {
                new Result
                {
                    Title    = display,
                    SubTitle = $"Copy result ({expr})",
                    Score    = 100,
                    Action   = _ =>
                    {
                        System.Windows.Clipboard.SetText(result?.ToString() ?? string.Empty);
                        return true;
                    }
                }
            };
        }
        catch
        {
            return Array.Empty<Result>();
        }
    }
}