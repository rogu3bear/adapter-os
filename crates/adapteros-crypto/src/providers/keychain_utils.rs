// Extracted keychain dict builder for Phase 41-50
// Citation: 【2025-11-12†refactor(crypto)†extract-keychain-dict】
use core_foundation::{base::CFType, dictionary::CFDictionary, data::CFData, string::CFString};
use security_framework_sys::item::{kSecClassGenericPassword, kSecAttrService, kSecAttrAccount, kSecValueData};

pub fn create_key_dict(service: &str, account: &str, data: Option<&[u8]>) -> CFDictionary<CFString, CFType> {
    let pairs = vec![
        (CFString::from(kSecClassGenericPassword), CFString::from("generic").as_concrete_type_ref()),
        (CFString::from(kSecAttrService), CFString::from(service).as_concrete_type_ref()),
        (CFString::from(kSecAttrAccount), CFString::from(account).as_concrete_type_ref()),
    ];
    if let Some(d) = data {
        pairs.push((CFString::from(kSecValueData), CFData::from_buffer_copy(d).as_concrete_type_ref()));
    }
    CFDictionary::from_CFType_pairs(&pairs)
}
