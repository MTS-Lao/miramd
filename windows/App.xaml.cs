using Microsoft.UI.Xaml;

namespace MiraMD.Windows;

public partial class App : Application
{
    private Window? _window;

    public App()
    {
        InitializeComponent();
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        var initialPath = Environment.GetCommandLineArgs()
            .Skip(1)
            .FirstOrDefault(path => File.Exists(path));
        _window = new MainWindow(initialPath);
        _window.Activate();
    }
}
