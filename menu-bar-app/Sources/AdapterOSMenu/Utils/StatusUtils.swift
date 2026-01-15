import Foundation
import AppKit

/// Utility functions for status-related operations
/// 【2025-11-07†refactor(swift)†extract-status-utils】
///
/// Consolidates duplicate utility functions from StatusMenuView:
/// - copyKernelHash
/// - copyStatusJSON
public struct StatusUtils {
    
    /// Copy kernel hash to pasteboard
    /// 【2025-11-07†refactor(swift)†extract-status-utils】
    ///
    /// - Parameter fullHash: The full kernel hash string to copy
    public static func copyKernelHash(_ fullHash: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(fullHash, forType: .string)
    }
    
    /// Copy status JSON to pasteboard
    /// 【2025-11-07†refactor(swift)†extract-status-utils】
    ///
    /// - Parameter status: The adapterOSStatus to encode and copy
    /// - Returns: Success status and optional error message
    public static func copyStatusJSON(_ status: adapterOSStatus) -> (success: Bool, error: String?) {
        do {
            let data = try JSONEncoder().encode(status)
            guard let string = String(data: data, encoding: .utf8) else {
                return (false, "Encoding failed")
            }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(string, forType: .string)
            return (true, nil)
        } catch {
            return (false, error.localizedDescription)
        }
    }
}

