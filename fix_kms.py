import os

def fix_kms_file():
    path = "/Users/star/Dev/adapter-os/crates/adapteros-crypto/src/providers/kms.rs"
    if not os.path.exists(path):
        return
        
    with open(path, 'r') as f:
        content = f.read()
    
    # Standardize to KmsProvider and provider field
    content = content.replace("KmsBackend", "KmsProvider")
    content = content.replace("KmsBackendType", "KmsProviderType")
    content = content.replace("LocalKmsBackend", "LocalKmsProvider")
    content = content.replace("MockKmsBackend", "MockKmsProvider")
    content = content.replace("GcpKmsBackend", "GcpKmsProvider")
    content = content.replace("HashicorpVaultBackend", "HashicorpVaultProvider")
    content = content.replace("self.backend.", "self.provider.")
    content = content.replace("self.backend(", "self.provider(")
    content = content.replace("self.backend,", "self.provider,")
    content = content.replace("self.backend ", "self.provider ")
    
    with open(path, 'w') as f:
        f.write(content)
        
def fix_gcp_file():
    path = "/Users/star/Dev/adapter-os/crates/adapteros-crypto/src/providers/gcp.rs"
    if not os.path.exists(path):
        return
        
    with open(path, 'r') as f:
        content = f.read()
        
    content = content.replace("KmsBackend", "KmsProvider")
    content = content.replace("KmsBackendType", "KmsProviderType")
    content = content.replace("GcpKmsBackend", "GcpKmsProvider")
    
    with open(path, 'w') as f:
        f.write(content)

if __name__ == "__main__":
    fix_kms_file()
    fix_gcp_file()
    print("Standardized KMS to KmsProvider and provider field.")
