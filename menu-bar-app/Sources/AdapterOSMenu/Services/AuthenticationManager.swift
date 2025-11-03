import Foundation
import Security

/// Manages authentication tokens securely using Keychain
final class AuthenticationManager {
    private let serviceName = "com.adapteros.menu"
    private let sharedSecretKey = "servicePanelSharedSecret"
    private let tokenKey = "servicePanelToken"
    private let tokenExpirationKey = "servicePanelTokenExpiration"

    private let tokenLifetime: TimeInterval = 3600 // 1 hour

    static let shared = AuthenticationManager()

    private init() {}

    // MARK: - Shared Secret Management

    func storeSharedSecret(_ secret: String) throws {
        let data = Data(secret.utf8)

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: sharedSecretKey,
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlock
        ]

        // Delete existing item if it exists
        SecItemDelete(query as CFDictionary)

        // Add new item
        let status = SecItemAdd(query as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw AuthenticationError.keychainError(status)
        }
    }

    func retrieveSharedSecret() throws -> String {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: sharedSecretKey,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]

        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)

        guard status == errSecSuccess else {
            if status == errSecItemNotFound {
                // Fall back to environment variable
                if let envSecret = ProcessInfo.processInfo.environment["SERVICE_PANEL_SECRET"] {
                    try storeSharedSecret(envSecret) // Cache it for next time
                    return envSecret
                }
                throw AuthenticationError.secretNotFound
            }
            throw AuthenticationError.keychainError(status)
        }

        guard let data = result as? Data,
              let secret = String(data: data, encoding: .utf8) else {
            throw AuthenticationError.invalidSecretData
        }

        return secret
    }

    // MARK: - Token Management

    func getOrCreateToken() throws -> String {
        // Check if we have a valid cached token
        if let cachedToken = try getCachedToken(), isTokenValid(cachedToken) {
            return cachedToken.token
        }

        // Create new token
        let secret = try retrieveSharedSecret()
        let credentials = "service-panel:\(secret)"
        let token = "Basic \(Data(credentials.utf8).base64EncodedString())"

        // Cache the token
        try cacheToken(token)

        return token
    }

    func invalidateToken() throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: tokenKey
        ]

        let status = SecItemDelete(query as CFDictionary)
        if status != errSecSuccess && status != errSecItemNotFound {
            throw AuthenticationError.keychainError(status)
        }

        // Also remove expiration
        let expirationQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: tokenExpirationKey
        ]

        SecItemDelete(expirationQuery as CFDictionary)
    }

    private func cacheToken(_ token: String) throws {
        let tokenData = Data(token.utf8)
        let expirationDate = Date().addingTimeInterval(tokenLifetime)

        // Store token
        let tokenQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: tokenKey,
            kSecValueData as String: tokenData,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlock
        ]

        SecItemDelete(tokenQuery as CFDictionary) // Delete existing
        let tokenStatus = SecItemAdd(tokenQuery as CFDictionary, nil)
        guard tokenStatus == errSecSuccess else {
            throw AuthenticationError.keychainError(tokenStatus)
        }

        // Store expiration
        let expirationData = Data("\(expirationDate.timeIntervalSince1970)".utf8)
        let expirationQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: tokenExpirationKey,
            kSecValueData as String: expirationData,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlock
        ]

        SecItemDelete(expirationQuery as CFDictionary) // Delete existing
        let expirationStatus = SecItemAdd(expirationQuery as CFDictionary, nil)
        guard expirationStatus == errSecSuccess else {
            throw AuthenticationError.keychainError(expirationStatus)
        }
    }

    private func getCachedToken() throws -> (token: String, expiration: Date)? {
        // Get token
        let tokenQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: tokenKey,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]

        var tokenResult: AnyObject?
        let tokenStatus = SecItemCopyMatching(tokenQuery as CFDictionary, &tokenResult)

        guard tokenStatus == errSecSuccess else {
            if tokenStatus == errSecItemNotFound {
                return nil
            }
            throw AuthenticationError.keychainError(tokenStatus)
        }

        guard let tokenData = tokenResult as? Data,
              let token = String(data: tokenData, encoding: .utf8) else {
            throw AuthenticationError.invalidTokenData
        }

        // Get expiration
        let expirationQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: tokenExpirationKey,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]

        var expirationResult: AnyObject?
        let expirationStatus = SecItemCopyMatching(expirationQuery as CFDictionary, &expirationResult)

        guard expirationStatus == errSecSuccess else {
            if expirationStatus == errSecItemNotFound {
                return nil
            }
            throw AuthenticationError.keychainError(expirationStatus)
        }

        guard let expirationData = expirationResult as? Data,
              let expirationString = String(data: expirationData, encoding: .utf8),
              let expirationTimestamp = TimeInterval(expirationString) else {
            throw AuthenticationError.invalidExpirationData
        }

        let expirationDate = Date(timeIntervalSince1970: expirationTimestamp)
        return (token: token, expiration: expirationDate)
    }

    private func isTokenValid(_ cachedToken: (token: String, expiration: Date)) -> Bool {
        return cachedToken.expiration > Date()
    }

    // MARK: - Utilities

    func clearAllStoredCredentials() throws {
        // Clear shared secret
        let secretQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: sharedSecretKey
        ]
        SecItemDelete(secretQuery as CFDictionary)

        // Clear token
        let tokenQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: tokenKey
        ]
        SecItemDelete(tokenQuery as CFDictionary)

        // Clear expiration
        let expirationQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: tokenExpirationKey
        ]
        SecItemDelete(expirationQuery as CFDictionary)
    }
}

enum AuthenticationError: Error, LocalizedError {
    case secretNotFound
    case invalidSecretData
    case invalidTokenData
    case invalidExpirationData
    case keychainError(OSStatus)

    var errorDescription: String? {
        switch self {
        case .secretNotFound:
            return "Shared secret not found in Keychain or environment"
        case .invalidSecretData:
            return "Invalid shared secret data in Keychain"
        case .invalidTokenData:
            return "Invalid token data in Keychain"
        case .invalidExpirationData:
            return "Invalid token expiration data in Keychain"
        case .keychainError(let status):
            return "Keychain error: \(status)"
        }
    }
}
