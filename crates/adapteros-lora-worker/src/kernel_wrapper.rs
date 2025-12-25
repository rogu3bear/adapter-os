//! Kernel wrapper types for backend execution with fallback support
//!
//! This module provides:
//! - StrictnessControl trait for managing strict mode and fallback behavior
//! - BackendLane enum for primary/fallback lane selection
//! - DirectKernels for single-backend execution
//! - CoordinatedKernels for dual-backend execution with fallback
//! - KernelWrapper enum unifying both execution strategies

use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

/// Strictness control for backend execution (strict mode disables fallback)
pub trait StrictnessControl {
    /// Set strict mode for subsequent operations
    fn set_strict_mode(&mut self, strict: bool);
    /// Reset fallback tracking for a new request
    fn reset_fallback(&mut self);
    /// Select active lane (primary/fallback) for next step
    fn set_active_lane(&mut self, lane: BackendLane);
    /// Report currently active lane
    fn active_lane(&self) -> BackendLane;
    /// Names for the available lanes (primary, fallback)
    fn lane_names(&self) -> (String, Option<String>);
    /// Whether fallback occurred on the last operation
    fn fallback_triggered(&self) -> bool;
    /// Backend name used on the last operation (if known)
    fn last_backend_used(&self) -> Option<String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendLane {
    Primary,
    Fallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveBackend {
    Primary,
    Fallback,
}

// Default strictness control for plain backends (no fallback)
impl StrictnessControl for Box<dyn FusedKernels + Send + Sync> {
    fn set_strict_mode(&mut self, _strict: bool) {}
    fn reset_fallback(&mut self) {}
    fn set_active_lane(&mut self, _lane: BackendLane) {}
    fn active_lane(&self) -> BackendLane {
        BackendLane::Primary
    }
    fn lane_names(&self) -> (String, Option<String>) {
        (self.device_name().to_string(), None)
    }
    fn fallback_triggered(&self) -> bool {
        false
    }
    fn last_backend_used(&self) -> Option<String> {
        Some(self.device_name().to_string())
    }
}

/// Direct single-backend wrapper (no fallback)
pub struct DirectKernels {
    inner: Box<dyn FusedKernels + Send + Sync>,
    last_backend: String,
}

impl DirectKernels {
    pub fn new(inner: Box<dyn FusedKernels + Send + Sync>) -> Self {
        let last_backend = inner.device_name().to_string();
        Self {
            inner,
            last_backend,
        }
    }
}

/// Coordinated backend wrapper with optional fallback backend
pub struct CoordinatedKernels {
    primary: Box<dyn FusedKernels + Send + Sync>,
    fallback: Option<Box<dyn FusedKernels + Send + Sync>>,
    active_backend: ActiveBackend,
    strict_mode: bool,
    primary_degraded: bool,
    fallback_triggered: bool,
    last_backend: String,
}

impl CoordinatedKernels {
    pub fn new(
        primary: Box<dyn FusedKernels + Send + Sync>,
        fallback: Option<Box<dyn FusedKernels + Send + Sync>>,
    ) -> Self {
        let last_backend = primary.device_name().to_string();
        Self {
            primary,
            fallback,
            active_backend: ActiveBackend::Primary,
            strict_mode: false,
            primary_degraded: false,
            fallback_triggered: false,
            last_backend,
        }
    }
}

/// Unified kernel wrapper supporting strictness control and optional fallback
pub enum KernelWrapper {
    Direct(DirectKernels),
    Coordinated(CoordinatedKernels),
}

impl StrictnessControl for KernelWrapper {
    fn set_strict_mode(&mut self, strict: bool) {
        if let KernelWrapper::Coordinated(k) = self {
            k.strict_mode = strict;
        }
    }

    fn reset_fallback(&mut self) {
        match self {
            KernelWrapper::Direct(k) => {
                k.last_backend = k.inner.device_name().to_string();
            }
            KernelWrapper::Coordinated(k) => {
                k.fallback_triggered = false;
                k.active_backend = if k.strict_mode || k.fallback.is_none() || !k.primary_degraded {
                    ActiveBackend::Primary
                } else {
                    ActiveBackend::Fallback
                };
                k.fallback_triggered = matches!(k.active_backend, ActiveBackend::Fallback);
                k.last_backend = match k.active_backend {
                    ActiveBackend::Primary => k.primary.device_name().to_string(),
                    ActiveBackend::Fallback => k
                        .fallback
                        .as_ref()
                        .map(|f| f.device_name().to_string())
                        .unwrap_or_else(|| k.primary.device_name().to_string()),
                };
            }
        }
    }

    fn set_active_lane(&mut self, lane: BackendLane) {
        match self {
            KernelWrapper::Direct(k) => {
                k.last_backend = k.inner.device_name().to_string();
            }
            KernelWrapper::Coordinated(k) => {
                match lane {
                    BackendLane::Primary => k.active_backend = ActiveBackend::Primary,
                    BackendLane::Fallback => {
                        if k.fallback.is_some() {
                            k.active_backend = ActiveBackend::Fallback;
                        } else {
                            k.active_backend = ActiveBackend::Primary;
                        }
                    }
                }
                k.fallback_triggered = matches!(k.active_backend, ActiveBackend::Fallback);
                k.last_backend = match k.active_backend {
                    ActiveBackend::Primary => k.primary.device_name().to_string(),
                    ActiveBackend::Fallback => k
                        .fallback
                        .as_ref()
                        .map(|f| f.device_name().to_string())
                        .unwrap_or_else(|| k.primary.device_name().to_string()),
                };
            }
        }
    }

    fn active_lane(&self) -> BackendLane {
        match self {
            KernelWrapper::Direct(_) => BackendLane::Primary,
            KernelWrapper::Coordinated(k) => match k.active_backend {
                ActiveBackend::Primary => BackendLane::Primary,
                ActiveBackend::Fallback => BackendLane::Fallback,
            },
        }
    }

    fn lane_names(&self) -> (String, Option<String>) {
        match self {
            KernelWrapper::Direct(k) => (k.inner.device_name().to_string(), None),
            KernelWrapper::Coordinated(k) => (
                k.primary.device_name().to_string(),
                k.fallback.as_ref().map(|f| f.device_name().to_string()),
            ),
        }
    }

    fn fallback_triggered(&self) -> bool {
        match self {
            KernelWrapper::Direct(_) => false,
            KernelWrapper::Coordinated(k) => k.fallback_triggered,
        }
    }

    fn last_backend_used(&self) -> Option<String> {
        match self {
            KernelWrapper::Direct(k) => Some(k.last_backend.clone()),
            KernelWrapper::Coordinated(k) => Some(k.last_backend.clone()),
        }
    }
}

impl FusedKernels for KernelWrapper {
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
        match self {
            KernelWrapper::Direct(k) => k.inner.load(plan_bytes),
            KernelWrapper::Coordinated(k) => {
                k.primary.load(plan_bytes)?;
                if let Some(fallback) = k.fallback.as_mut() {
                    fallback.load(plan_bytes)?;
                }
                Ok(())
            }
        }
    }

    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        match self {
            KernelWrapper::Direct(k) => k.inner.run_step(ring, io),
            KernelWrapper::Coordinated(k) => {
                k.fallback_triggered = matches!(k.active_backend, ActiveBackend::Fallback);
                match k.active_backend {
                    ActiveBackend::Primary => match k.primary.run_step(ring, io) {
                        Ok(_) => {
                            k.primary_degraded = false;
                            k.last_backend = k.primary.device_name().to_string();
                            k.fallback_triggered = false;
                            Ok(())
                        }
                        Err(e) => {
                            k.primary_degraded = true;
                            k.last_backend = k.primary.device_name().to_string();
                            Err(e)
                        }
                    },
                    ActiveBackend::Fallback => {
                        let Some(fallback) = k.fallback.as_mut() else {
                            return Err(AosError::Kernel(
                                "Fallback backend not configured".to_string(),
                            ));
                        };

                        match fallback.run_step(ring, io) {
                            Ok(_) => {
                                k.last_backend = fallback.device_name().to_string();
                                k.fallback_triggered = true;
                                Ok(())
                            }
                            Err(e) => {
                                k.last_backend = fallback.device_name().to_string();
                                Err(e)
                            }
                        }
                    }
                }
            }
        }
    }

    fn device_name(&self) -> &str {
        match self {
            KernelWrapper::Direct(k) => k.inner.device_name(),
            KernelWrapper::Coordinated(k) => k.last_backend.as_str(),
        }
    }

    fn attest_determinism(
        &self,
    ) -> Result<adapteros_lora_kernel_api::attestation::DeterminismReport> {
        match self {
            KernelWrapper::Direct(k) => k.inner.attest_determinism(),
            KernelWrapper::Coordinated(k) => k.primary.attest_determinism(),
        }
    }

    fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()> {
        match self {
            KernelWrapper::Direct(k) => k.inner.load_adapter(id, weights),
            KernelWrapper::Coordinated(k) => {
                k.primary.load_adapter(id, weights)?;
                if let Some(fallback) = k.fallback.as_mut() {
                    fallback.load_adapter(id, weights)?;
                }
                Ok(())
            }
        }
    }

    fn unload_adapter(&mut self, id: u16) -> Result<()> {
        match self {
            KernelWrapper::Direct(k) => k.inner.unload_adapter(id),
            KernelWrapper::Coordinated(k) => {
                k.primary.unload_adapter(id)?;
                if let Some(fallback) = k.fallback.as_mut() {
                    fallback.unload_adapter(id)?;
                }
                Ok(())
            }
        }
    }

    fn attach_adapter(&mut self, id: u16) -> Result<()> {
        match self {
            KernelWrapper::Direct(k) => k.inner.attach_adapter(id),
            KernelWrapper::Coordinated(k) => {
                if let Some(fallback) = k.fallback.as_mut() {
                    fallback.attach_adapter(id)?;
                }
                k.primary.attach_adapter(id)
            }
        }
    }

    fn detach_adapter(&mut self, id: u16) -> Result<()> {
        match self {
            KernelWrapper::Direct(k) => k.inner.detach_adapter(id),
            KernelWrapper::Coordinated(k) => {
                if let Some(fallback) = k.fallback.as_mut() {
                    fallback.detach_adapter(id)?;
                }
                k.primary.detach_adapter(id)
            }
        }
    }

    fn switch_adapter(&mut self, id: u16) -> Result<()> {
        match self {
            KernelWrapper::Direct(k) => k.inner.switch_adapter(id),
            KernelWrapper::Coordinated(k) => {
                if let Some(fallback) = k.fallback.as_mut() {
                    fallback.switch_adapter(id)?;
                }
                k.primary.switch_adapter(id)
            }
        }
    }

    fn supports_streaming_text_generation(&self) -> bool {
        match self {
            KernelWrapper::Direct(k) => k.inner.supports_streaming_text_generation(),
            KernelWrapper::Coordinated(k) => k.primary.supports_streaming_text_generation(),
        }
    }

    fn generate_text_complete(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
    ) -> Result<adapteros_lora_kernel_api::TextGenerationResult> {
        match self {
            KernelWrapper::Direct(k) => {
                k.inner
                    .generate_text_complete(prompt, max_tokens, temperature, top_p)
            }
            KernelWrapper::Coordinated(k) => {
                k.primary
                    .generate_text_complete(prompt, max_tokens, temperature, top_p)
            }
        }
    }
}
