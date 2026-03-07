//! Reference Metal vision kernels.
//!
//! The production implementation would execute on Apple Silicon GPUs via the
//! Metal shading language.  For the open-source worker we provide a
//! deterministic CPU reference implementation that mirrors the behaviour of
//! the Metal kernels.  The interface is intentionally kept simple so that the
//! worker crate can integrate the kernels without taking a direct dependency on
//! Metal at compile time on non-macOS platforms.

use adapteros_core::{AosError, B3Hash, Result};

/// Supported convolution architectures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetalVisionArchitecture {
    ResNet,
    Vgg,
    EfficientNet,
}

/// Activation function applied after each convolution stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MetalVisionActivation {
    Relu,
    LeakyRelu { slope: f32 },
    None,
}

/// Pooling strategy used after the convolution stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetalVisionPooling {
    Max,
    Average,
    None,
}

/// Configuration provided to the Metal vision kernels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetalVisionKernelConfig {
    pub architecture: MetalVisionArchitecture,
    pub activation: MetalVisionActivation,
    pub pooling: MetalVisionPooling,
    pub apply_batch_norm: bool,
}

/// Borrowed tensor view in NCHW format.
#[derive(Debug, Clone, Copy)]
pub struct MetalImageTensor<'a> {
    pub data: &'a [f32],
    pub batch: usize,
    pub channels: usize,
    pub height: usize,
    pub width: usize,
}

impl<'a> MetalImageTensor<'a> {
    pub fn new(
        data: &'a [f32],
        batch: usize,
        channels: usize,
        height: usize,
        width: usize,
    ) -> Result<Self> {
        let expected = batch
            .checked_mul(channels)
            .and_then(|v| v.checked_mul(height))
            .and_then(|v| v.checked_mul(width))
            .ok_or_else(|| AosError::Validation("tensor dimensions overflow".into()))?;
        if data.len() != expected {
            return Err(AosError::Validation(format!(
                "tensor has {} elements but expected {}",
                data.len(),
                expected
            )));
        }
        Ok(Self {
            data,
            batch,
            channels,
            height,
            width,
        })
    }
}

/// Owned tensor resulting from kernel execution.
#[derive(Debug, Clone)]
pub struct MetalImageTensorOwned {
    pub data: Vec<f32>,
    pub batch: usize,
    pub channels: usize,
    pub height: usize,
    pub width: usize,
}

impl MetalImageTensorOwned {
    pub fn into_parts(self) -> (Vec<f32>, usize, usize, usize, usize) {
        (
            self.data,
            self.batch,
            self.channels,
            self.height,
            self.width,
        )
    }
}

/// Bundle of vision kernels.  On macOS this would wrap the underlying Metal
/// pipeline.  For non-macOS targets the implementation simply returns an error
/// so that the worker can fall back to the pure Rust pipeline.
pub struct VisionKernelBundle;

#[cfg(not(target_os = "macos"))]
impl VisionKernelBundle {
    pub fn convolve(
        _input: MetalImageTensor<'_>,
        _config: MetalVisionKernelConfig,
    ) -> Result<MetalImageTensorOwned> {
        Err(AosError::Kernel(
            "Metal vision kernels require macOS".to_string(),
        ))
    }
}

#[cfg(target_os = "macos")]
impl VisionKernelBundle {
    pub fn convolve(
        input: MetalImageTensor<'_>,
        config: MetalVisionKernelConfig,
    ) -> Result<MetalImageTensorOwned> {
        let layers = match config.architecture {
            MetalVisionArchitecture::ResNet => resnet_layers(),
            MetalVisionArchitecture::Vgg => vgg_layers(),
            MetalVisionArchitecture::EfficientNet => efficientnet_layers(),
        };

        let mut current = ImageBatch::from_tensor(input);
        for layer in layers {
            current = layer.forward(&current, config.activation)?;
        }

        current = match config.pooling {
            MetalVisionPooling::None => current,
            MetalVisionPooling::Max => max_pool(&current)?,
            MetalVisionPooling::Average => avg_pool(&current)?,
        };

        if config.apply_batch_norm {
            current = batch_normalize(&current)?;
        }

        Ok(current.into_owned())
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
struct ImageBatch {
    data: Vec<f32>,
    batch: usize,
    channels: usize,
    height: usize,
    width: usize,
}

#[cfg(target_os = "macos")]
impl ImageBatch {
    fn from_tensor(tensor: MetalImageTensor<'_>) -> Self {
        Self {
            data: tensor.data.to_vec(),
            batch: tensor.batch,
            channels: tensor.channels,
            height: tensor.height,
            width: tensor.width,
        }
    }

    fn zeros(batch: usize, channels: usize, height: usize, width: usize) -> Self {
        Self {
            data: vec![0.0; batch * channels * height * width],
            batch,
            channels,
            height,
            width,
        }
    }

    fn index(&self, n: usize, c: usize, h: usize, w: usize) -> usize {
        (((n * self.channels + c) * self.height + h) * self.width) + w
    }

    fn into_owned(self) -> MetalImageTensorOwned {
        MetalImageTensorOwned {
            data: self.data,
            batch: self.batch,
            channels: self.channels,
            height: self.height,
            width: self.width,
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
struct ConvLayer {
    weights: Vec<f32>,
    bias: Vec<f32>,
    in_channels: usize,
    out_channels: usize,
    kernel_size: usize,
    stride: usize,
    padding: usize,
}

#[cfg(target_os = "macos")]
impl ConvLayer {
    fn forward(&self, input: &ImageBatch, activation: MetalVisionActivation) -> Result<ImageBatch> {
        if input.channels != self.in_channels {
            return Err(AosError::Validation("channel mismatch".into()));
        }

        let out_height = ((input.height + 2 * self.padding - self.kernel_size) / self.stride) + 1;
        let out_width = ((input.width + 2 * self.padding - self.kernel_size) / self.stride) + 1;
        let mut output = ImageBatch::zeros(input.batch, self.out_channels, out_height, out_width);

        for n in 0..input.batch {
            for oc in 0..self.out_channels {
                for oh in 0..out_height {
                    for ow in 0..out_width {
                        let mut acc = self.bias[oc];
                        for ic in 0..self.in_channels {
                            for kh in 0..self.kernel_size {
                                for kw in 0..self.kernel_size {
                                    let ih = (oh * self.stride + kh).wrapping_sub(self.padding);
                                    let iw = (ow * self.stride + kw).wrapping_sub(self.padding);
                                    if ih < input.height && iw < input.width {
                                        let idx = input.index(n, ic, ih, iw);
                                        let weight_idx = (((oc * self.in_channels + ic)
                                            * self.kernel_size
                                            + kh)
                                            * self.kernel_size)
                                            + kw;
                                        acc += input.data[idx] * self.weights[weight_idx];
                                    }
                                }
                            }
                        }

                        let activated = match activation {
                            MetalVisionActivation::Relu => acc.max(0.0),
                            MetalVisionActivation::LeakyRelu { slope } => {
                                if acc >= 0.0 {
                                    acc
                                } else {
                                    acc * slope
                                }
                            }
                            MetalVisionActivation::None => acc,
                        };
                        let out_idx = output.index(n, oc, oh, ow);
                        output.data[out_idx] = activated;
                    }
                }
            }
        }

        Ok(output)
    }
}

#[cfg(target_os = "macos")]
fn resnet_layers() -> Vec<ConvLayer> {
    vec![
        make_layer(3, 16, 3, 1, 1, "mtl_resnet_layer1"),
        make_layer(16, 32, 3, 2, 1, "mtl_resnet_layer2"),
        make_layer(32, 64, 3, 2, 1, "mtl_resnet_layer3"),
    ]
}

#[cfg(target_os = "macos")]
fn vgg_layers() -> Vec<ConvLayer> {
    vec![
        make_layer(3, 32, 3, 1, 1, "mtl_vgg_layer1"),
        make_layer(32, 64, 3, 1, 1, "mtl_vgg_layer2"),
        make_layer(64, 64, 3, 2, 1, "mtl_vgg_layer3"),
    ]
}

#[cfg(target_os = "macos")]
fn efficientnet_layers() -> Vec<ConvLayer> {
    vec![
        make_layer(3, 24, 3, 1, 1, "mtl_eff_layer1"),
        make_layer(24, 24, 3, 1, 1, "mtl_eff_layer2"),
        make_layer(24, 40, 5, 2, 2, "mtl_eff_layer3"),
    ]
}

#[cfg(target_os = "macos")]
fn make_layer(
    in_channels: usize,
    out_channels: usize,
    kernel_size: usize,
    stride: usize,
    padding: usize,
    salt: &str,
) -> ConvLayer {
    let weights = deterministic_weights(in_channels, out_channels, kernel_size, salt);
    let bias = deterministic_bias(out_channels, salt);
    ConvLayer {
        weights,
        bias,
        in_channels,
        out_channels,
        kernel_size,
        stride,
        padding,
    }
}

#[cfg(target_os = "macos")]
fn deterministic_weights(
    in_channels: usize,
    out_channels: usize,
    kernel_size: usize,
    salt: &str,
) -> Vec<f32> {
    let total = out_channels * in_channels * kernel_size * kernel_size;
    let mut weights = Vec::with_capacity(total);
    let hash = B3Hash::hash(salt.as_bytes());
    let bytes = hash.as_bytes();

    for i in 0..total {
        let byte = bytes[i % bytes.len()] as f32;
        let value = ((byte / 255.0) - 0.5) * 0.1;
        weights.push(value);
    }

    weights
}

#[cfg(target_os = "macos")]
fn deterministic_bias(out_channels: usize, salt: &str) -> Vec<f32> {
    let hash = B3Hash::hash(format!("{}_bias", salt).as_bytes());
    let bytes = hash.as_bytes();
    (0..out_channels)
        .map(|i| ((bytes[i % bytes.len()] as f32) / 255.0 - 0.5) * 0.05)
        .collect()
}

#[cfg(target_os = "macos")]
fn max_pool(batch: &ImageBatch) -> Result<ImageBatch> {
    if batch.height < 2 || batch.width < 2 {
        return Ok(batch.clone());
    }
    let out_height = batch.height / 2;
    let out_width = batch.width / 2;
    let mut output = ImageBatch::zeros(batch.batch, batch.channels, out_height, out_width);

    for n in 0..batch.batch {
        for c in 0..batch.channels {
            for oh in 0..out_height {
                for ow in 0..out_width {
                    let mut max_val = f32::NEG_INFINITY;
                    for kh in 0..2 {
                        for kw in 0..2 {
                            let ih = oh * 2 + kh;
                            let iw = ow * 2 + kw;
                            let idx = batch.index(n, c, ih, iw);
                            max_val = max_val.max(batch.data[idx]);
                        }
                    }
                    let out_idx = output.index(n, c, oh, ow);
                    output.data[out_idx] = max_val;
                }
            }
        }
    }

    Ok(output)
}

#[cfg(target_os = "macos")]
fn avg_pool(batch: &ImageBatch) -> Result<ImageBatch> {
    if batch.height < 2 || batch.width < 2 {
        return Ok(batch.clone());
    }
    let out_height = batch.height / 2;
    let out_width = batch.width / 2;
    let mut output = ImageBatch::zeros(batch.batch, batch.channels, out_height, out_width);

    for n in 0..batch.batch {
        for c in 0..batch.channels {
            for oh in 0..out_height {
                for ow in 0..out_width {
                    let mut sum = 0.0;
                    for kh in 0..2 {
                        for kw in 0..2 {
                            let ih = oh * 2 + kh;
                            let iw = ow * 2 + kw;
                            let idx = batch.index(n, c, ih, iw);
                            sum += batch.data[idx];
                        }
                    }
                    let out_idx = output.index(n, c, oh, ow);
                    output.data[out_idx] = sum / 4.0;
                }
            }
        }
    }

    Ok(output)
}

#[cfg(target_os = "macos")]
fn batch_normalize(batch: &ImageBatch) -> Result<ImageBatch> {
    let mut normalized = batch.clone();
    let elements_per_channel = batch.batch * batch.height * batch.width;
    if elements_per_channel == 0 {
        return Err(AosError::Validation("invalid tensor dimensions".into()));
    }

    for c in 0..batch.channels {
        let mut sum = 0.0;
        let mut sum_sq = 0.0;
        for n in 0..batch.batch {
            for h in 0..batch.height {
                for w in 0..batch.width {
                    let idx = batch.index(n, c, h, w);
                    let value = batch.data[idx];
                    sum += value;
                    sum_sq += value * value;
                }
            }
        }

        let mean = sum / elements_per_channel as f32;
        let variance = (sum_sq / elements_per_channel as f32) - mean * mean;
        let variance = variance.max(0.0);
        let inv_std = 1.0 / (variance.sqrt() + 1e-5);

        for n in 0..batch.batch {
            for h in 0..batch.height {
                for w in 0..batch.width {
                    let idx = batch.index(n, c, h, w);
                    let normalized_value = (batch.data[idx] - mean) * inv_std;
                    normalized.data[idx] = normalized_value;
                }
            }
        }
    }

    Ok(normalized)
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::*;

    #[test]
    fn test_resnet_kernel_shape() {
        let input = vec![0.0f32; 1 * 3 * 32 * 32];
        let tensor = MetalImageTensor::new(&input, 1, 3, 32, 32).unwrap();
        let config = MetalVisionKernelConfig {
            architecture: MetalVisionArchitecture::ResNet,
            activation: MetalVisionActivation::Relu,
            pooling: MetalVisionPooling::Max,
            apply_batch_norm: true,
        };
        let output = VisionKernelBundle::convolve(tensor, config).unwrap();
        assert_eq!(output.channels, 64);
    }
}
