import Foundation
import CryptoKit

/// Runtime loader for design tokens from JSON resource
final class DesignTokensLoader {
    static let shared = DesignTokensLoader()
    
    private let resourceName = "DesignTokens"
    private let resourceExtension = "json"
    
    private var cachedTokens: DesignTokensModel?
    private var cachedChecksum: String?
    private var isDegradedMode: Bool = false
    
    private init() {
        loadTokens()
    }
    
    // MARK: - Public API
    
    /// Current design tokens (fallback to defaults if loading failed)
    var tokens: DesignTokensModel {
        cachedTokens ?? .defaults
    }
    
    /// SHA256 checksum of the loaded tokens file (empty if using defaults)
    var checksum: String {
        cachedChecksum ?? ""
    }
    
    /// Whether the loader is operating in degraded mode (using defaults)
    var degraded: Bool {
        isDegradedMode
    }
    
    /// Reload tokens from resource file
    func reload() {
        loadTokens()
    }
    
    // MARK: - Private Implementation
    
    private func loadTokens() {
        // Try Bundle.module first (SPM auto-generated), fallback to Bundle.main (executable)
        let bundle = Bundle.module // SPM auto-generates this
        guard let url = bundle.url(forResource: resourceName, withExtension: resourceExtension) ??
                        Bundle.main.url(forResource: resourceName, withExtension: resourceExtension) else {
            Logger.shared.warning(
                "DesignTokens.json not found in bundle, using defaults",
                context: ["resource": "\(resourceName).\(resourceExtension)"]
            )
            cachedTokens = nil
            cachedChecksum = nil
            isDegradedMode = true
            return
        }
        
        do {
            let data = try Data(contentsOf: url)
            
            // Calculate checksum
            let hash = SHA256.hash(data: data)
            cachedChecksum = hash.compactMap { String(format: "%02x", $0) }.joined()
            
            // Decode JSON
            let decoder = JSONDecoder()
            let decoded = try decoder.decode(DesignTokensModel.self, from: data)
            
            cachedTokens = decoded
            isDegradedMode = false
            
            Logger.shared.info(
                "Design tokens loaded successfully",
                context: [
                    "resource": "\(resourceName).\(resourceExtension)",
                    "checksum": cachedChecksum ?? "unknown"
                ]
            )
        } catch {
            Logger.shared.error(
                "Failed to load design tokens, using defaults",
                error: error,
                context: [
                    "resource": "\(resourceName).\(resourceExtension)",
                    "error_type": String(describing: type(of: error))
                ]
            )
            cachedTokens = nil
            cachedChecksum = nil
            isDegradedMode = true
        }
    }
}

