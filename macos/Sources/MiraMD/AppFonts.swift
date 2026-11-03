import AppKit
import CoreText
import SwiftUI

@MainActor
enum AppFonts {
    static let laoFamily = "Phetsarath Lao"

    static let resourceBundle: Bundle = {
        if let resources = Bundle.main.resourceURL,
           let packaged = Bundle(url: resources.appendingPathComponent("MiraMD_MiraMD.bundle")) {
            return packaged
        }
        return Bundle.module
    }()

    private static let registration: Void = {
        for name in ["PhetsarathLao-Regular", "PhetsarathLao-SemiBold", "PhetsarathLao-Bold"] {
            guard let url = resourceBundle.url(forResource: name, withExtension: "woff2", subdirectory: "Fonts") else {
                continue
            }
            var error: Unmanaged<CFError>?
            CTFontManagerRegisterFontsForURL(url as CFURL, .process, &error)
            if let error {
                let value = error.takeRetainedValue() as Error
                NSLog("MiraMD could not register %@: %@", name, value.localizedDescription)
            }
        }
    }()

    static func register() {
        _ = registration
    }

    static func lao(size: CGFloat, weight: NSFont.Weight = .regular) -> NSFont {
        register()
        let postScriptName: String
        if weight >= .bold {
            postScriptName = "PhetsarathLao-Bold"
        } else if weight >= .semibold {
            postScriptName = "PhetsarathLao-SemiBold"
        } else {
            postScriptName = "PhetsarathLao-Regular"
        }
        return NSFont(name: postScriptName, size: size)
            ?? NSFont(name: laoFamily, size: size)
            ?? .systemFont(ofSize: size, weight: weight)
    }

    static func containsLao(_ value: String) -> Bool {
        value.unicodeScalars.contains { (0x0E80...0x0EFF).contains(Int($0.value)) }
    }
}
