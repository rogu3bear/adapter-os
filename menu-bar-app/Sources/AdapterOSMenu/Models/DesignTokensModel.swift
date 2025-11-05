import Foundation
import SwiftUI

/// Runtime design tokens model loaded from JSON
struct DesignTokensModel: Codable {
    let colors: ColorTokens
    let fonts: FontTokens
    let spacing: SpacingTokens
    
    struct ColorTokens: Codable {
        let ok: String
        let degraded: String
        let error: String
        let surface: String?
        
        enum CodingKeys: String, CodingKey {
            case ok, degraded, error, surface
        }
    }
    
    struct FontTokens: Codable {
        let header: FontDescriptor
        let metrics: FontDescriptor
        
        struct FontDescriptor: Codable {
            let name: String
            let size: CGFloat
            let weight: String?
            let monospaced: Bool?
            
            enum CodingKeys: String, CodingKey {
                case name, size, weight, monospaced
            }
        }
    }
    
    struct SpacingTokens: Codable {
        let xs: CGFloat
        let sm: CGFloat
        let md: CGFloat
        let lg: CGFloat
        let xl: CGFloat
        
        enum CodingKeys: String, CodingKey {
            case xs, sm, md, lg, xl
        }
    }
}

// MARK: - SwiftUI Conversion Helpers

extension DesignTokensModel.ColorTokens {
    /// Convert hex color string to SwiftUI Color
    func toColor(_ hex: String) -> Color {
        let hex = hex.trimmingCharacters(in: CharacterSet.alphanumerics.inverted)
        let a, r, g, b: UInt64
        
        switch hex.count {
        case 3: // RGB (12-bit, e.g., "#0F0" -> #00FF00)
            // Parse each character separately: R, G, B
            let chars = Array(hex)
            var rHex: UInt64 = 0
            var gHex: UInt64 = 0
            var bHex: UInt64 = 0
            if chars.count > 0 { Scanner(string: String(chars[0])).scanHexInt64(&rHex) }
            if chars.count > 1 { Scanner(string: String(chars[1])).scanHexInt64(&gHex) }
            if chars.count > 2 { Scanner(string: String(chars[2])).scanHexInt64(&bHex) }
            (a, r, g, b) = (255, rHex * 17, gHex * 17, bHex * 17)
        case 6: // RGB (24-bit, e.g., "#00FF00")
            var int: UInt64 = 0
            Scanner(string: hex).scanHexInt64(&int)
            (a, r, g, b) = (255, int >> 16, (int >> 8) & 0xFF, int & 0xFF)
        case 8: // ARGB (32-bit, e.g., "#FF00FF00")
            var int: UInt64 = 0
            Scanner(string: hex).scanHexInt64(&int)
            (a, r, g, b) = ((int >> 24) & 0xFF, (int >> 16) & 0xFF, (int >> 8) & 0xFF, int & 0xFF)
        default:
            return .gray // Fallback for invalid hex
        }
        return Color(
            .sRGB,
            red: Double(r) / 255,
            green: Double(g) / 255,
            blue: Double(b) / 255,
            opacity: Double(a) / 255
        )
    }
    
    var okColor: Color { toColor(ok) }
    var degradedColor: Color { toColor(degraded) }
    var errorColor: Color { toColor(error) }
    var surfaceColor: Color {
        if let surface = surface {
            return toColor(surface)
        }
        return Color(nsColor: .windowBackgroundColor)
    }
}

extension DesignTokensModel.FontTokens.FontDescriptor {
    /// Convert font descriptor to SwiftUI Font
    func toFont() -> Font {
        // Determine font weight
        let fontWeight: Font.Weight = {
            guard let weight = weight else { return .regular }
            switch weight.lowercased() {
            case "ultralight", "thin":
                return .ultraLight
            case "light":
                return .light
            case "regular", "normal":
                return .regular
            case "medium":
                return .medium
            case "semibold":
                return .semibold
            case "bold":
                return .bold
            case "heavy", "black":
                return .heavy
            default:
                return .regular
            }
        }()
        
        // Map font name to system font if needed
        let font: Font
        switch name.lowercased() {
        case "system", "sf pro", "san francisco":
            font = .system(size: size, weight: fontWeight)
        case "monospaced", "monaco", "menlo":
            // Monospaced system font with weight
            font = .system(size: size, weight: fontWeight, design: .monospaced)
        default:
            // Custom font - weight may be part of font name or not supported
            font = .custom(name, size: size)
        }
        
        // Apply monospaced digits if specified
        if monospaced == true {
            return font.monospacedDigit()
        }
        
        return font
    }
}

extension DesignTokensModel.FontTokens {
    var headerFont: Font { header.toFont() }
    var metricsFont: Font { metrics.toFont() }
}

// MARK: - Default Tokens

extension DesignTokensModel {
    /// Default design tokens used as fallback
    static var defaults: DesignTokensModel {
        DesignTokensModel(
            colors: ColorTokens(
                ok: "#34C759",
                degraded: "#FF9500",
                error: "#FF3B30",
                surface: nil
            ),
            fonts: FontTokens(
                header: FontTokens.FontDescriptor(
                    name: "system",
                    size: 16,
                    weight: nil,
                    monospaced: false
                ),
                metrics: FontTokens.FontDescriptor(
                    name: "system",
                    size: 12,
                    weight: nil,
                    monospaced: true
                )
            ),
            spacing: SpacingTokens(
                xs: 2,
                sm: 4,
                md: 8,
                lg: 12,
                xl: 16
            )
        )
    }
}

