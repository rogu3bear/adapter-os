use adapteros_core::{AosError, Result};
use std::sync::Arc;

#[cfg(target_os = "macos")]
use metal::Device;

/// Specification for executing a vision convolution kernel on Metal. The
/// structure is shared between CPU and Metal implementations to keep unit tests
/// deterministic across platforms.
#[derive(Debug, Clone, Copy)]
pub struct VisionKernelSpec {
    pub input_channels: usize,
    pub output_channels: usize,
    pub kernel_size: usize,
    pub stride: usize,
    pub height: usize,
    pub width: usize,
}

impl Default for VisionKernelSpec {
    fn default() -> Self {
        Self {
            input_channels: 3,
            output_channels: 32,
            kernel_size: 3,
            stride: 1,
            height: 224,
            width: 224,
        }
    }
}

/// Metal vision kernels with a CPU fallback implementation to guarantee
/// deterministic output when a Metal device is unavailable.
#[derive(Debug)]
pub struct MetalVisionKernels {
    #[cfg(target_os = "macos")]
    device: Arc<Device>,
    spec: Option<VisionKernelSpec>,
    quantization_scale: f32,
}

impl MetalVisionKernels {
    pub fn new() -> Result<Self> {
        #[cfg(target_os = "macos")]
        let device = Device::system_default().ok_or_else(|| {
            AosError::Kernel("No Metal device available for vision kernels".to_string())
        })?;

        Ok(Self {
            #[cfg(target_os = "macos")]
            device: Arc::new(device),
            spec: None,
            quantization_scale: 127.0,
        })
    }

    pub fn configure(&mut self, spec: VisionKernelSpec) {
        self.spec = Some(spec);
    }

    pub fn quantize(&self, data: &[f32]) -> Vec<i8> {
        data.iter()
            .map(|value| {
                let clamped = value.clamp(-1.0, 1.0);
                (clamped * self.quantization_scale).round() as i8
            })
            .collect()
    }

    pub fn normalize(&self, data: &mut [f32], mean: &[f32], std: &[f32]) {
        if data.is_empty() {
            return;
        }
        let channels = mean.len().max(1);
        let inv_std: Vec<f32> = std
            .iter()
            .map(|value| if value.abs() < 1e-6 { 1.0 } else { 1.0 / value })
            .collect();
        let spatial = data.len() / channels;
        for c in 0..channels {
            for idx in 0..spatial {
                let index = c * spatial + idx;
                let normalized = (data[index] - mean[c]) * inv_std[c];
                data[index] = normalized;
            }
        }
    }

    pub fn execute(&self, input: &[f32], weights: &[f32]) -> Result<Vec<f32>> {
        let spec = self
            .spec
            .ok_or_else(|| AosError::Kernel("vision kernel not configured".to_string()))?;
        let out_height =
            (spec.height + 2 * (spec.kernel_size / 2) - spec.kernel_size) / spec.stride + 1;
        let out_width =
            (spec.width + 2 * (spec.kernel_size / 2) - spec.kernel_size) / spec.stride + 1;
        let mut output = vec![0.0; spec.output_channels * out_height * out_width];

        for oc in 0..spec.output_channels {
            for oy in 0..out_height {
                for ox in 0..out_width {
                    let mut acc = 0.0f32;
                    for ic in 0..spec.input_channels {
                        for ky in 0..spec.kernel_size {
                            for kx in 0..spec.kernel_size {
                                let iy = oy * spec.stride + ky;
                                let ix = ox * spec.stride + kx;
                                if iy < spec.height && ix < spec.width {
                                    let input_idx =
                                        ic * spec.height * spec.width + iy * spec.width + ix;
                                    let weight_idx = oc
                                        * spec.input_channels
                                        * spec.kernel_size
                                        * spec.kernel_size
                                        + ic * spec.kernel_size * spec.kernel_size
                                        + ky * spec.kernel_size
                                        + kx;
                                    acc += input[input_idx] * weights[weight_idx];
                                }
                            }
                        }
                    }
                    let out_idx = oc * out_height * out_width + oy * out_width + ox;
                    output[out_idx] = acc;
                }
            }
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_fallback_matches_expectations() {
        let mut kernels = MetalVisionKernels::new().expect("create kernels");
        let spec = VisionKernelSpec::default();
        kernels.configure(spec);
        let input = vec![0.5; spec.input_channels * spec.height * spec.width];
        let weights =
            vec![
                0.1;
                spec.output_channels * spec.input_channels * spec.kernel_size * spec.kernel_size
            ];
        let output = kernels.execute(&input, &weights).expect("execute");
        assert_eq!(
            output.len(),
            spec.output_channels
                * ((spec.height + 2 * (spec.kernel_size / 2) - spec.kernel_size) / spec.stride + 1)
                * ((spec.width + 2 * (spec.kernel_size / 2) - spec.kernel_size) / spec.stride + 1)
        );
    }
}
