# AdapterOS Version Information

## Current Version: alpha-v0.01-1

**Release Date**: January 15, 2025  
**Status**: Alpha Release  
**Stability**: Development  

### Version History

| Version | Date | Status | Description |
|---------|------|--------|-------------|
| alpha-v0.01-1 | 2025-01-15 | Alpha | Initial alpha release with core features |

### Versioning Scheme

AdapterOS uses semantic versioning with alpha/beta/rc prefixes:

- **alpha-vX.Y.Z**: Alpha releases for development and testing
- **beta-vX.Y.Z**: Beta releases for pre-production testing
- **rc-vX.Y.Z**: Release candidates for final testing
- **vX.Y.Z**: Stable releases for production use

### Current Alpha Features

#### ✅ Completed (alpha-v0.01-1)
- Naming unification (`mplora-*` → `adapteros-*`)
- Policy registry with 20 canonical packs
- Metal kernel refactor with modular design
- Deterministic configuration system
- Database schema lifecycle management
- GitHub repository setup and documentation

#### 🔄 In Progress
- Server API structural refactoring
- Integration test suite completion
- API reference documentation
- Deployment guides and examples

#### 📋 Planned (v0.02)
- Performance optimization and calibration
- Security hardening and threat detection
- Comprehensive monitoring and observability
- Enterprise deployment features

### Compatibility

#### Backward Compatibility
- Compatibility shims provided for `mplora-*` crates
- Deprecation warnings for old crate names
- One release cycle support for migration

#### Forward Compatibility
- Policy registry designed for extensibility
- Configuration schema supports new fields
- Database migrations handle schema evolution

### Upgrade Path

#### From Development Versions
1. Update crate dependencies to `adapteros-*` names
2. Run database migrations: `aosctl db migrate`
3. Update configuration files to new schema
4. Test with compatibility shims before full migration

#### To Future Versions
- Policy packs may be added but not removed
- Configuration schema will be extended, not reduced
- Database migrations will be provided for all changes

### Support Policy

#### Alpha Releases
- **Support**: Community support via GitHub issues
- **Stability**: Not guaranteed, breaking changes possible
- **Updates**: Frequent updates with new features
- **Documentation**: Basic documentation, may be incomplete

#### Beta Releases
- **Support**: Community support with faster response
- **Stability**: API stability with deprecation warnings
- **Updates**: Regular updates with bug fixes
- **Documentation**: Complete documentation

#### Stable Releases
- **Support**: Full support with security updates
- **Stability**: API stability with semantic versioning
- **Updates**: Security and bug fix updates
- **Documentation**: Complete documentation and guides

### Security Updates

Security updates will be provided for:
- **Alpha**: Critical security vulnerabilities
- **Beta**: Critical and high severity vulnerabilities
- **Stable**: All security vulnerabilities

### Release Schedule

#### Alpha Phase (v0.01.x)
- **Duration**: 2-3 months
- **Frequency**: Weekly releases
- **Focus**: Core feature development and testing

#### Beta Phase (v0.02.x)
- **Duration**: 1-2 months
- **Frequency**: Bi-weekly releases
- **Focus**: Stability, performance, and documentation

#### Stable Phase (v1.0.0+)
- **Duration**: Ongoing
- **Frequency**: Monthly releases
- **Focus**: Security updates, bug fixes, and minor features

### Contact

For version-related questions or issues:
- **GitHub Issues**: [rogu3bear/adapter-os](https://github.com/rogu3bear/adapter-os/issues)
- **Email**: vats-springs0m@icloud.com
- **Discussions**: GitHub Discussions for community support

---

**Last Updated**: January 15, 2025  
**Next Review**: February 15, 2025
