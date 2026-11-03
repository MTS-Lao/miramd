import SwiftUI

enum AppAppearance: String, CaseIterable, Identifiable {
    case system
    case light
    case dark

    var id: Self { self }
    var title: String { rawValue.capitalized }
    var icon: String {
        switch self {
        case .system: "circle.lefthalf.filled"
        case .light: "sun.max"
        case .dark: "moon"
        }
    }
    var colorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }
}

@MainActor
final class AppearanceController: ObservableObject {
    @Published var selection: AppAppearance {
        didSet { UserDefaults.standard.set(selection.rawValue, forKey: Self.preferenceKey) }
    }

    private static let preferenceKey = "MiraMD.appearance"

    init() {
        let stored = UserDefaults.standard.string(forKey: Self.preferenceKey)
        selection = stored.flatMap(AppAppearance.init(rawValue:)) ?? .system
    }
}
