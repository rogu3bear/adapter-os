//! Deterministic convolution pipeline for vision adapters.
//!
//! The production implementation would delegate the heavy lifting to
//! architecture specific Metal kernels.  For the purposes of the open-source
//! adapter worker we provide a fully deterministic CPU reference
//! implementation that mirrors the behaviour of those kernels.  The pipeline
//! is capable of processing image batches, applying multiple convolution
//! stages, pooling and normalization in a reproducible manner across
//! platforms.  When running on macOS the pipeline will opportunistically
//! offload the convolution to the Metal vision kernels implemented in the
//! `adapteros-lora-kernel-mtl` crate.  On other platforms the pure Rust
//! implementation is used.

use adapteros_core::{AosError, B3Hash, Result};

#[cfg(target_os = "macos")]
use adapteros_lora_kernel_mtl::vision_kernels::{
    MetalVisionActivation, MetalVisionArchitecture, MetalVisionPooling,
};

/// Simple NCHW tensor used by the worker side vision adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageBatch {
    /// Flattened tensor data in NCHW layout.
    pub data: Vec<f32>,
    /// Batch size (N).
    pub batch: usize,
    /// Number of channels (C).
    pub channels: usize,
    /// Image height (H).
    pub height: usize,
    /// Image width (W).
    pub width: usize,
}

impl ImageBatch {
    /// Create a new batch tensor, validating that the provided data matches
    /// the expected number of elements.
    pub fn new(
        data: Vec<f32>,
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
                "tensor has {} elements but {} expected for {}x{}x{}x{}",
                data.len(),
                expected,
                batch,
                channels,
                height,
                width
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

    /// Convenience method to create a tensor filled with zeros.
    pub fn zeros(batch: usize, channels: usize, height: usize, width: usize) -> Self {
        let len = batch * channels * height * width;
        Self {
            data: vec![0.0; len],
            batch,
            channels,
            height,
            width,
        }
    }

    #[inline]
    fn index(&self, n: usize, c: usize, h: usize, w: usize) -> usize {
        (((n * self.channels + c) * self.height + h) * self.width) + w
    }
}

/// Supported convolution backbones.  The exact kernel shapes are kept simple
/// but deterministic.  The enum mirrors common computer vision networks which
/// allows downstream components to select the appropriate distribution of
/// channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionArchitecture {
    ResNet,
    Vgg,
    EfficientNet,
}

/// Pooling strategy applied after the convolution stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolingStrategy {
    /// 2×2 max pooling with stride 2.
    Max,
    /// 2×2 average pooling with stride 2.
    Average,
    /// Skip pooling.
    None,
}

/// Activation function applied after each convolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActivationKind {
    Relu,
    LeakyRelu(f32),
    None,
}

/// Configuration for the convolution pipeline.
#[derive(Debug, Clone)]
pub struct ConvPipelineConfig {
    pub architecture: VisionArchitecture,
    pub activation: ActivationKind,
    pub pooling: PoolingStrategy,
    pub apply_batch_norm: bool,
    pub prefer_metal: bool,
}

impl Default for ConvPipelineConfig {
    fn default() -> Self {
        Self {
            architecture: VisionArchitecture::ResNet,
            activation: ActivationKind::Relu,
            pooling: PoolingStrategy::Max,
            apply_batch_norm: true,
            prefer_metal: false,
        }
    }
}

/// Single convolution layer description.
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

impl ConvLayer {
    fn forward(&self, input: &ImageBatch, activation: ActivationKind) -> Result<ImageBatch> {
        if input.channels != self.in_channels {
            return Err(AosError::Validation(format!(
                "expected {} input channels, got {}",
                self.in_channels, input.channels
            )));
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
                                        let input_idx = input.index(n, ic, ih, iw);
                                        let weight_idx = (((oc * self.in_channels + ic)
                                            * self.kernel_size
                                            + kh)
                                            * self.kernel_size)
                                            + kw;
                                        acc += input.data[input_idx] * self.weights[weight_idx];
                                    }
                                }
                            }
                        }

                        let activated = match activation {
                            ActivationKind::Relu => acc.max(0.0),
                            ActivationKind::LeakyRelu(alpha) => {
                                if acc >= 0.0 {
                                    acc
                                } else {
                                    acc * alpha
                                }
                            }
                            ActivationKind::None => acc,
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

/// Convolution pipeline capable of processing image batches deterministically.
#[derive(Debug)]
pub struct ConvPipeline {
    config: ConvPipelineConfig,
    layers: Vec<ConvLayer>,
}

impl ConvPipeline {
    /// Construct a new pipeline using the supplied configuration.
    pub fn new(config: ConvPipelineConfig) -> Self {
        let layers = match config.architecture {
            VisionArchitecture::ResNet => Self::resnet_layers(),
            VisionArchitecture::Vgg => Self::vgg_layers(),
            VisionArchitecture::EfficientNet => Self::efficientnet_layers(),
        };

        Self { config, layers }
    }

    /// Process an image batch, returning the transformed tensor.  The function
    /// is deterministic regardless of the execution backend.
    pub fn process_batch(&self, batch: &ImageBatch) -> Result<ImageBatch> {
        if batch.batch == 0 {
            return Err(AosError::Validation("empty batch".into()));
        }

        #[cfg(target_os = "macos")]
        if self.config.prefer_metal {
            if let Some(result) = self.try_metal(batch) {
                return result;
            }
        }

        let mut current = batch.clone();
        for layer in &self.layers {
            current = layer.forward(&current, self.config.activation)?;
        }

        current = match self.config.pooling {
            PoolingStrategy::None => current,
            PoolingStrategy::Max => Self::max_pool(&current)?,
            PoolingStrategy::Average => Self::avg_pool(&current)?,
        };

        if self.config.apply_batch_norm {
            current = Self::batch_normalize(&current)?;
        }

        Ok(current)
    }

    #[cfg(target_os = "macos")]
    fn try_metal(&self, batch: &ImageBatch) -> Option<Result<ImageBatch>> {
        use adapteros_lora_kernel_mtl::vision_kernels::{
            MetalImageTensor,
            MetalVisionKernelConfig, VisionKernelBundle,
        };

        let tensor = match MetalImageTensor::new(
            &batch.data,
            batch.batch,
            batch.channels,
            batch.height,
            batch.width,
        ) {
            Ok(tensor) => tensor,
            Err(err) => return Some(Err(err)),
        };

        let config = MetalVisionKernelConfig {
            architecture: self.config.architecture.into(),
            activation: self.config.activation.into(),
            pooling: self.config.pooling.into(),
            apply_batch_norm: self.config.apply_batch_norm,
        };

        match VisionKernelBundle::convolve(tensor, config) {
            Ok(result) => {
                let (data, batch_size, channels, height, width) = result.into_parts();
                match ImageBatch::new(data, batch_size, channels, height, width) {
                    Ok(batch) => Some(Ok(batch)),
                    Err(err) => Some(Err(err)),
                }
            }
            Err(err) => {
                tracing::warn!("Falling back to CPU convolution pipeline: {}", err);
                None
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn try_metal(&self, _batch: &ImageBatch) -> Option<Result<ImageBatch>> {
        None
    }

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

    fn resnet_layers() -> Vec<ConvLayer> {
        let mut layers = Vec::new();
        layers.push(Self::make_layer(3, 16, 3, 1, 1, "resnet_layer1"));
        layers.push(Self::make_layer(16, 32, 3, 2, 1, "resnet_layer2"));
        layers.push(Self::make_layer(32, 64, 3, 2, 1, "resnet_layer3"));
        layers
    }

    fn vgg_layers() -> Vec<ConvLayer> {
        let mut layers = Vec::new();
        layers.push(Self::make_layer(3, 32, 3, 1, 1, "vgg_layer1"));
        layers.push(Self::make_layer(32, 64, 3, 1, 1, "vgg_layer2"));
        layers.push(Self::make_layer(64, 64, 3, 2, 1, "vgg_layer3"));
        layers
    }

    fn efficientnet_layers() -> Vec<ConvLayer> {
        let mut layers = Vec::new();
        layers.push(Self::make_layer(3, 24, 3, 1, 1, "efficient_layer1"));
        layers.push(Self::make_layer(24, 24, 3, 1, 1, "efficient_layer2"));
        layers.push(Self::make_layer(24, 40, 5, 2, 2, "efficient_layer3"));
        layers
    }

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
}

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
        let value = ((byte / 255.0) - 0.5) * 0.1; // small deterministic weights
        weights.push(value);
    }

    weights
}

fn deterministic_bias(out_channels: usize, salt: &str) -> Vec<f32> {
    let hash = B3Hash::hash(format!("{}_bias", salt).as_bytes());
    let bytes = hash.as_bytes();
    (0..out_channels)
        .map(|i| ((bytes[i % bytes.len()] as f32) / 255.0 - 0.5) * 0.05)
        .collect()
}

#[cfg(target_os = "macos")]
impl From<VisionArchitecture> for MetalVisionArchitecture {
    fn from(value: VisionArchitecture) -> Self {
        match value {
            VisionArchitecture::ResNet => MetalVisionArchitecture::ResNet,
            VisionArchitecture::Vgg => MetalVisionArchitecture::Vgg,
            VisionArchitecture::EfficientNet => MetalVisionArchitecture::EfficientNet,
        }
    }
}

#[cfg(target_os = "macos")]
impl From<ActivationKind> for MetalVisionActivation {
    fn from(value: ActivationKind) -> Self {
        match value {
            ActivationKind::Relu => MetalVisionActivation::Relu,
            ActivationKind::LeakyRelu(alpha) => MetalVisionActivation::LeakyRelu { slope: alpha },
            ActivationKind::None => MetalVisionActivation::None,
        }
    }
}

#[cfg(target_os = "macos")]
impl From<PoolingStrategy> for MetalVisionPooling {
    fn from(value: PoolingStrategy) -> Self {
        match value {
            PoolingStrategy::Max => MetalVisionPooling::Max,
            PoolingStrategy::Average => MetalVisionPooling::Average,
            PoolingStrategy::None => MetalVisionPooling::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_batch(batch: usize, channels: usize, height: usize, width: usize) -> ImageBatch {
        let total = batch * channels * height * width;
        let data: Vec<f32> = (0..total).map(|i| (i % 255) as f32 / 255.0).collect();
        ImageBatch::new(data, batch, channels, height, width).unwrap()
    }

    #[test]
    fn test_resnet_pipeline_shapes() {
        let pipeline = ConvPipeline::new(ConvPipelineConfig {
            architecture: VisionArchitecture::ResNet,
            ..Default::default()
        });
        let batch = create_test_batch(2, 3, 32, 32);
        let output = pipeline.process_batch(&batch).unwrap();
        assert_eq!(output.batch, 2);
        assert_eq!(output.channels, 64);
        assert_eq!(output.height, 8);
        assert_eq!(output.width, 8);
    }

    #[test]
    fn test_vgg_pipeline_shapes() {
        let pipeline = ConvPipeline::new(ConvPipelineConfig {
            architecture: VisionArchitecture::Vgg,
            pooling: PoolingStrategy::Average,
            ..Default::default()
        });
        let batch = create_test_batch(1, 3, 32, 32);
        let output = pipeline.process_batch(&batch).unwrap();
        assert_eq!(output.channels, 64);
        assert_eq!(output.height, 16);
        assert_eq!(output.width, 16);
    }

    #[test]
    fn test_efficientnet_pipeline_shapes() {
        let pipeline = ConvPipeline::new(ConvPipelineConfig {
            architecture: VisionArchitecture::EfficientNet,
            pooling: PoolingStrategy::None,
            ..Default::default()
        });
        let batch = create_test_batch(1, 3, 64, 64);
        let output = pipeline.process_batch(&batch).unwrap();
        assert_eq!(output.channels, 40);
        assert_eq!(output.height, 16);
        assert_eq!(output.width, 16);
    }

    #[test]
    fn test_batch_norm_determinism() {
        let pipeline = ConvPipeline::new(ConvPipelineConfig {
            architecture: VisionArchitecture::ResNet,
            ..Default::default()
        });
        let batch = create_test_batch(1, 3, 16, 16);
        let out1 = pipeline.process_batch(&batch).unwrap();
        let out2 = pipeline.process_batch(&batch).unwrap();
        assert_eq!(out1.data, out2.data);
    }
}
