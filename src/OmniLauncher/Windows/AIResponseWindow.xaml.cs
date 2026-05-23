using System.Windows;

namespace OmniLauncher.Windows;

public partial class AIResponseWindow : Window
{
    private readonly string _response;

    public AIResponseWindow(string prompt, string response)
    {
        InitializeComponent();
        _response = response;
        PromptLabel.Text = $"Prompt: {prompt}";
        ResponseText.Text = response;
    }

    private void CopyBtn_Click(object sender, RoutedEventArgs e) => Clipboard.SetText(_response);
    private void CloseBtn_Click(object sender, RoutedEventArgs e) => Close();
}