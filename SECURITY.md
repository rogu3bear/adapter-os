# Security Policy

## Supported Versions

We actively support the following versions with security updates:

| Version | Supported          |
| ------- | ------------------ |
| 0.12.x  | :white_check_mark: |
| 0.11.x  | :white_check_mark: |
| < 0.11  | :x:                |

## Reporting a Vulnerability

If you discover a security vulnerability in adapterOS, please report it to us as follows:

### Contact
- **Email**: security@adapteros.ai
- **Response Time**: We will acknowledge receipt within 48 hours
- **Updates**: We will provide regular updates on the progress of fixing the vulnerability

### What to Include
Please include the following information in your report:
- A clear description of the vulnerability
- Steps to reproduce the issue
- Potential impact and severity
- Any suggested fixes or mitigations

### Our Commitment
- We will investigate all legitimate reports
- We will keep you informed about our progress
- We will credit you (if desired) once the issue is resolved
- We will not pursue legal action against security researchers

## Security Considerations

adapterOS implements several security measures:

### Deterministic Execution
- Cryptographic seeding prevents timing attacks
- Deterministic computation ensures reproducible results
- HKDF key derivation for all randomness

### Policy Enforcement
- 25 canonical security policies
- Runtime policy validation
- Audit logging for all operations

### Network Security
- Zero network egress during inference
- Unix domain socket communication
- Air-gapped operation support

### Memory Safety
- Rust memory safety guarantees
- Bounds checking and overflow protection
- Secure FFI interfaces

## Security Testing

We maintain comprehensive security testing including:

- Automated security regression tests
- Fuzz testing for input validation
- Memory safety verification
- Cryptographic primitive testing

## Responsible Disclosure

We kindly ask that you:
- Give us reasonable time to fix the issue before public disclosure
- Avoid accessing user data or disrupting services
- Report vulnerabilities in a responsible manner

Thank you for helping keep adapterOS and its users secure!
