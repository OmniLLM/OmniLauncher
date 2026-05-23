namespace OmniLauncher.Core;

/// <summary>
/// Every plugin must implement this interface.
/// </summary>
public interface IPlugin
{
    string Name { get; }
    string Description { get; }
    /// <summary>Optional keyword prefix (e.g. "g " for Google search).</summary>
    string? Keyword { get; }

    void Init(PluginInitContext context);
    IList<Result> Query(Query query);
}

public record PluginInitContext(string PluginDirectory, IPublicAPI API);

public interface IPublicAPI
{
    void ChangeQuery(string query, bool requery = false);
    void HideMainWindow();
    void ShowMainWindow();
}

public record Query(string RawQuery, string Search, string ActionKeyword);

public record Result
{
    public required string Title { get; init; }
    public string? SubTitle { get; init; }
    public string? IcoPath { get; init; }
    public int Score { get; init; } = 0;
    public Func<ActionContext, bool>? Action { get; init; }
}

public record ActionContext(bool SpecialKeyState);