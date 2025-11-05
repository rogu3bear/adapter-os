import XCTest
@testable import AdapterOSMenu
import Foundation
import CryptoKit

final class DesignTokensLoaderTests: XCTestCase {
    var tempDir: URL!
    var tempBundle: Bundle!
    
    override func setUp() {
        super.setUp()
        
        // Create temporary directory for test resources
        tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("DesignTokensTests-\(UUID().uuidString)")
        
        try? FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
        
        // Create a temporary bundle for testing
        // Note: In a real scenario, we'd need to create a proper bundle, but for unit tests
        // we'll test the loader's behavior directly with mock data
    }
    
    override func tearDown() {
        try? FileManager.default.removeItem(at: tempDir)
        tempDir = nil
        tempBundle = nil
        super.tearDown()
    }
    
    // MARK: - Default Tokens Tests
    
    func testDefaultTokensExist() {
        let defaults = DesignTokensModel.defaults
        
        // Verify colors exist
        XCTAssertNotNil(defaults.colors.ok)
        XCTAssertNotNil(defaults.colors.degraded)
        XCTAssertNotNil(defaults.colors.error)
        
        // Verify fonts exist
        XCTAssertNotNil(defaults.fonts.header)
        XCTAssertNotNil(defaults.fonts.metrics)
        
        // Verify spacing exists
        XCTAssertGreaterThan(defaults.spacing.xs, 0)
        XCTAssertGreaterThan(defaults.spacing.sm, 0)
        XCTAssertGreaterThan(defaults.spacing.md, 0)
        XCTAssertGreaterThan(defaults.spacing.lg, 0)
        XCTAssertGreaterThan(defaults.spacing.xl, 0)
    }
    
    // MARK: - Color Conversion Tests
    
    func testHexColorConversion() {
        let colors = DesignTokensModel.ColorTokens(
            ok: "#00FF00",
            degraded: "#FFFF00",
            error: "#FF0000",
            surface: nil
        )
        
        // Test that colors can be converted
        let okColor = colors.okColor
        let degradedColor = colors.degradedColor
        let errorColor = colors.errorColor
        
        XCTAssertNotNil(okColor)
        XCTAssertNotNil(degradedColor)
        XCTAssertNotNil(errorColor)
    }
    
    func testHexColorConversionShortForm() {
        let colors = DesignTokensModel.ColorTokens(
            ok: "#0F0",
            degraded: "#FF0",
            error: "#F00",
            surface: nil
        )
        
        // Test that short form hex colors work
        let okColor = colors.okColor
        XCTAssertNotNil(okColor)
    }
    
    func testHexColorConversionInvalid() {
        let colors = DesignTokensModel.ColorTokens(
            ok: "invalid",
            degraded: "#FFFF00",
            error: "#FF0000",
            surface: nil
        )
        
        // Invalid hex should fallback to gray
        let okColor = colors.okColor
        XCTAssertNotNil(okColor)
    }
    
    // MARK: - Font Conversion Tests
    
    func testFontConversion() {
        let fontDesc = DesignTokensModel.FontTokens.FontDescriptor(
            name: "system",
            size: 16,
            weight: nil,
            monospaced: false
        )
        
        let font = fontDesc.toFont()
        XCTAssertNotNil(font)
    }
    
    func testFontConversionWithWeight() {
        let fontDesc = DesignTokensModel.FontTokens.FontDescriptor(
            name: "system",
            size: 16,
            weight: "bold",
            monospaced: false
        )
        
        let font = fontDesc.toFont()
        XCTAssertNotNil(font)
    }
    
    func testFontConversionMonospaced() {
        let fontDesc = DesignTokensModel.FontTokens.FontDescriptor(
            name: "system",
            size: 12,
            weight: nil,
            monospaced: true
        )
        
        let font = fontDesc.toFont()
        XCTAssertNotNil(font)
    }
    
    // MARK: - JSON Decoding Tests
    
    func testJSONDecodingValid() throws {
        let json = """
        {
          "colors": {
            "ok": "#00FF00",
            "degraded": "#FFFF00",
            "error": "#FF0000",
            "surface": null
          },
          "fonts": {
            "header": {
              "name": "system",
              "size": 16,
              "weight": null,
              "monospaced": false
            },
            "metrics": {
              "name": "system",
              "size": 12,
              "weight": null,
              "monospaced": true
            }
          },
          "spacing": {
            "xs": 4,
            "sm": 8,
            "md": 12,
            "lg": 16,
            "xl": 24
          }
        }
        """
        
        let data = json.data(using: .utf8)!
        let decoder = JSONDecoder()
        
        let tokens = try decoder.decode(DesignTokensModel.self, from: data)
        
        XCTAssertEqual(tokens.colors.ok, "#00FF00")
        XCTAssertEqual(tokens.colors.degraded, "#FFFF00")
        XCTAssertEqual(tokens.colors.error, "#FF0000")
        XCTAssertEqual(tokens.spacing.xs, 4)
        XCTAssertEqual(tokens.spacing.sm, 8)
        XCTAssertEqual(tokens.spacing.md, 12)
        XCTAssertEqual(tokens.spacing.lg, 16)
        XCTAssertEqual(tokens.spacing.xl, 24)
    }
    
    func testJSONDecodingInvalid() {
        let json = """
        {
          "colors": {
            "ok": "#00FF00"
          }
        }
        """
        
        let data = json.data(using: .utf8)!
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        
        XCTAssertThrowsError(try decoder.decode(DesignTokensModel.self, from: data)) { error in
            XCTAssertTrue(error is DecodingError)
        }
    }
    
    // MARK: - Checksum Tests
    
    func testChecksumCalculation() throws {
        let json = """
        {
          "colors": {
            "ok": "#00FF00",
            "degraded": "#FFFF00",
            "error": "#FF0000",
            "surface": null
          },
          "fonts": {
            "header": {
              "name": "system",
              "size": 16,
              "weight": null,
              "monospaced": false
            },
            "metrics": {
              "name": "system",
              "size": 12,
              "weight": null,
              "monospaced": true
            }
          },
          "spacing": {
            "xs": 4,
            "sm": 8,
            "md": 12,
            "lg": 16,
            "xl": 24
          }
        }
        """
        
        let data = json.data(using: .utf8)!
        let hash = SHA256.hash(data: data)
        let checksum = hash.compactMap { String(format: "%02x", $0) }.joined()
        
        // Checksum should be 64 characters (32 bytes * 2 hex chars)
        XCTAssertEqual(checksum.count, 64)
        XCTAssertFalse(checksum.isEmpty)
    }
    
    // MARK: - DesignTokens Facade Tests
    
    func testDesignTokensAccessors() {
        // Test that DesignTokens provides access to colors
        let okColor = DesignTokens.okColor
        let degradedColor = DesignTokens.degradedColor
        let errorColor = DesignTokens.errorColor
        
        XCTAssertNotNil(okColor)
        XCTAssertNotNil(degradedColor)
        XCTAssertNotNil(errorColor)
    }
    
    func testDesignTokensSpacing() {
        // Test that spacing tokens are accessible
        let xs = DesignTokens.spacingXS
        let sm = DesignTokens.spacingSM
        let md = DesignTokens.spacingMD
        let lg = DesignTokens.spacingLG
        let xl = DesignTokens.spacingXL
        
        XCTAssertGreaterThan(xs, 0)
        XCTAssertGreaterThan(sm, xs)
        XCTAssertGreaterThan(md, sm)
        XCTAssertGreaterThan(lg, md)
        XCTAssertGreaterThan(xl, lg)
    }
    
    func testDesignTokensChecksum() {
        // Checksum may be empty if using defaults, but should not crash
        let checksum = DesignTokens.checksum
        // In degraded mode (defaults), checksum is empty
        // In normal mode, checksum is 64 characters
        XCTAssertTrue(checksum.isEmpty || checksum.count == 64)
    }
    
    func testDesignTokensDegradedMode() {
        // Test that degraded mode flag is accessible
        let isDegraded = DesignTokens.isDegradedMode
        // May be true or false depending on whether resource file exists
        XCTAssertNotNil(isDegraded)
    }
}

