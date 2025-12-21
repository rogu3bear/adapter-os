import SwiftUI

<<<<<<< HEAD
/// Design tokens facade backed by runtime loader
enum DesignTokens {
    private static let loader = DesignTokensLoader.shared
    
    // MARK: - Colors
    
    static var okColor: Color {
        loader.tokens.colors.okColor
    }
    
    static var degradedColor: Color {
        loader.tokens.colors.degradedColor
    }
    
    static var errorColor: Color {
        loader.tokens.colors.errorColor
    }
    
    static var surface: Color {
        loader.tokens.colors.surfaceColor
    }
    
    // MARK: - Fonts
    
    static var headerFont: Font {
        loader.tokens.fonts.headerFont
    }
    
    static var metricsFont: Font {
        loader.tokens.fonts.metricsFont
    }
    
    // MARK: - Spacing
    
    static var spacingXS: CGFloat {
        loader.tokens.spacing.xs
    }
    
    static var spacingSM: CGFloat {
        loader.tokens.spacing.sm
    }
    
    static var spacingMD: CGFloat {
        loader.tokens.spacing.md
    }
    
    static var spacingLG: CGFloat {
        loader.tokens.spacing.lg
    }
    
    static var spacingXL: CGFloat {
        loader.tokens.spacing.xl
    }
    
    // MARK: - Metadata
    
    /// SHA256 checksum of loaded tokens file (empty if using defaults)
    static var checksum: String {
        loader.checksum
    }
    
    /// Whether tokens are loaded from file (false if using defaults)
    static var isDegradedMode: Bool {
        loader.degraded
    }
    
    /// Reload tokens from resource file
    static func reload() {
        loader.reload()
    }
=======
enum DesignTokens {
    static let okColor: Color = .green
    static let degradedColor: Color = .yellow
    static let errorColor: Color = .red
    static let surface: Color = Color(nsColor: .secondarySystemBackground)
    static let headerFont: Font = .headline
    static let metricsFont: Font = .subheadline.monospacedDigit()
>>>>>>> integration-branch
}


