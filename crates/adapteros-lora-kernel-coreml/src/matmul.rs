//! Matrix multiplication utilities using Apple Accelerate framework
//!
//! Provides fast matrix multiplication using vDSP/BLAS for hybrid CoreML + LoRA inference.
//! Uses cblas_sgemm for optimal performance on Apple Silicon.

use adapteros_core::{AosError, Result};

/// Matrix multiply using Apple Accelerate (vDSP/BLAS)
///
/// Computes C = A @ B where:
/// - A is [M, K] row-major
/// - B is [K, N] row-major
/// - C is [M, N] row-major
///
/// Uses cblas_sgemm for optimal performance on Apple Silicon.
#[cfg(target_os = "macos")]
pub fn matmul_accelerate(
    a: &[f32], // [M, K]
    b: &[f32], // [K, N]
    m: usize,
    k: usize,
    n: usize,
) -> Result<Vec<f32>> {
    if a.len() != m * k {
        return Err(AosError::Kernel(format!(
            "Matrix A size mismatch: expected {} ({}x{}), got {}",
            m * k, m, k, a.len()
        )));
    }
    if b.len() != k * n {
        return Err(AosError::Kernel(format!(
            "Matrix B size mismatch: expected {} ({}x{}), got {}",
            k * n, k, n, b.len()
        )));
    }

    let mut c = vec![0.0f32; m * n];

    unsafe {
        // cblas_sgemm(order, transA, transB, M, N, K, alpha, A, lda, B, ldb, beta, C, ldc)
        // C = alpha * op(A) * op(B) + beta * C
        cblas_sgemm(
            CblasRowMajor,
            CblasNoTrans,
            CblasNoTrans,
            m as i32,
            n as i32,
            k as i32,
            1.0,            // alpha
            a.as_ptr(),
            k as i32,       // lda (leading dimension of A)
            b.as_ptr(),
            n as i32,       // ldb (leading dimension of B)
            0.0,            // beta
            c.as_mut_ptr(),
            n as i32,       // ldc (leading dimension of C)
        );
    }

    Ok(c)
}

/// Fallback for non-macOS platforms
#[cfg(not(target_os = "macos"))]
pub fn matmul_accelerate(
    a: &[f32],
    b: &[f32],
    m: usize,
    k: usize,
    n: usize,
) -> Result<Vec<f32>> {
    matmul_naive(a, b, m, k, n)
}

/// Naive matrix multiplication fallback
///
/// For testing and non-macOS platforms.
pub fn matmul_naive(
    a: &[f32], // [M, K]
    b: &[f32], // [K, N]
    m: usize,
    k: usize,
    n: usize,
) -> Result<Vec<f32>> {
    if a.len() != m * k {
        return Err(AosError::Kernel(format!(
            "Matrix A size mismatch: expected {}, got {}",
            m * k, a.len()
        )));
    }
    if b.len() != k * n {
        return Err(AosError::Kernel(format!(
            "Matrix B size mismatch: expected {}, got {}",
            k * n, b.len()
        )));
    }

    let mut c = vec![0.0f32; m * n];

    for i in 0..m {
        for j in 0..n {
            let mut sum = 0.0f32;
            for l in 0..k {
                sum += a[i * k + l] * b[l * n + j];
            }
            c[i * n + j] = sum;
        }
    }

    Ok(c)
}

/// Matrix-vector multiply: y = A @ x
///
/// Where A is [M, N] and x is [N], produces y [M].
#[cfg(target_os = "macos")]
pub fn matvec_accelerate(
    a: &[f32], // [M, N] row-major
    x: &[f32], // [N]
    m: usize,
    n: usize,
) -> Result<Vec<f32>> {
    if a.len() != m * n {
        return Err(AosError::Kernel(format!(
            "Matrix A size mismatch: expected {}, got {}",
            m * n, a.len()
        )));
    }
    if x.len() != n {
        return Err(AosError::Kernel(format!(
            "Vector x size mismatch: expected {}, got {}",
            n, x.len()
        )));
    }

    let mut y = vec![0.0f32; m];

    unsafe {
        // cblas_sgemv(order, trans, M, N, alpha, A, lda, X, incX, beta, Y, incY)
        // Y = alpha * A * X + beta * Y
        cblas_sgemv(
            CblasRowMajor,
            CblasNoTrans,
            m as i32,
            n as i32,
            1.0,           // alpha
            a.as_ptr(),
            n as i32,      // lda
            x.as_ptr(),
            1,             // incX
            0.0,           // beta
            y.as_mut_ptr(),
            1,             // incY
        );
    }

    Ok(y)
}

#[cfg(not(target_os = "macos"))]
pub fn matvec_accelerate(
    a: &[f32],
    x: &[f32],
    m: usize,
    n: usize,
) -> Result<Vec<f32>> {
    matvec_naive(a, x, m, n)
}

/// Naive matrix-vector multiply fallback
pub fn matvec_naive(
    a: &[f32], // [M, N]
    x: &[f32], // [N]
    m: usize,
    n: usize,
) -> Result<Vec<f32>> {
    if a.len() != m * n {
        return Err(AosError::Kernel(format!(
            "Matrix A size mismatch: expected {}, got {}",
            m * n, a.len()
        )));
    }
    if x.len() != n {
        return Err(AosError::Kernel(format!(
            "Vector x size mismatch: expected {}, got {}",
            n, x.len()
        )));
    }

    let mut y = vec![0.0f32; m];

    for i in 0..m {
        let mut sum = 0.0f32;
        for j in 0..n {
            sum += a[i * n + j] * x[j];
        }
        y[i] = sum;
    }

    Ok(y)
}

/// Scaled vector addition: y = y + alpha * x
///
/// In-place update of y.
pub fn axpy(alpha: f32, x: &[f32], y: &mut [f32]) -> Result<()> {
    if x.len() != y.len() {
        return Err(AosError::Kernel(format!(
            "Vector size mismatch: x={}, y={}",
            x.len(), y.len()
        )));
    }

    #[cfg(target_os = "macos")]
    unsafe {
        cblas_saxpy(
            x.len() as i32,
            alpha,
            x.as_ptr(),
            1,
            y.as_mut_ptr(),
            1,
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        for (yi, xi) in y.iter_mut().zip(x.iter()) {
            *yi += alpha * xi;
        }
    }

    Ok(())
}

// =============================================================================
// Accelerate Framework FFI bindings (macOS only)
// =============================================================================

#[cfg(target_os = "macos")]
#[allow(non_upper_case_globals)]
const CblasRowMajor: i32 = 101;

#[cfg(target_os = "macos")]
#[allow(non_upper_case_globals)]
const CblasNoTrans: i32 = 111;

#[cfg(target_os = "macos")]
#[link(name = "Accelerate", kind = "framework")]
extern "C" {
    fn cblas_sgemm(
        order: i32,
        transA: i32,
        transB: i32,
        M: i32,
        N: i32,
        K: i32,
        alpha: f32,
        A: *const f32,
        lda: i32,
        B: *const f32,
        ldb: i32,
        beta: f32,
        C: *mut f32,
        ldc: i32,
    );

    fn cblas_sgemv(
        order: i32,
        trans: i32,
        M: i32,
        N: i32,
        alpha: f32,
        A: *const f32,
        lda: i32,
        X: *const f32,
        incX: i32,
        beta: f32,
        Y: *mut f32,
        incY: i32,
    );

    fn cblas_saxpy(
        N: i32,
        alpha: f32,
        X: *const f32,
        incX: i32,
        Y: *mut f32,
        incY: i32,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matmul_2x3_3x2() {
        // A = [[1, 2, 3], [4, 5, 6]] (2x3)
        // B = [[7, 8], [9, 10], [11, 12]] (3x2)
        // C = A @ B = [[58, 64], [139, 154]] (2x2)
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let b = vec![7.0, 8.0, 9.0, 10.0, 11.0, 12.0];

        let c = matmul_accelerate(&a, &b, 2, 3, 2).unwrap();

        assert_eq!(c.len(), 4);
        assert!((c[0] - 58.0).abs() < 1e-5);
        assert!((c[1] - 64.0).abs() < 1e-5);
        assert!((c[2] - 139.0).abs() < 1e-5);
        assert!((c[3] - 154.0).abs() < 1e-5);
    }

    #[test]
    fn test_matvec_3x2() {
        // A = [[1, 2], [3, 4], [5, 6]] (3x2)
        // x = [1, 2]
        // y = A @ x = [5, 11, 17]
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let x = vec![1.0, 2.0];

        let y = matvec_accelerate(&a, &x, 3, 2).unwrap();

        assert_eq!(y.len(), 3);
        assert!((y[0] - 5.0).abs() < 1e-5);
        assert!((y[1] - 11.0).abs() < 1e-5);
        assert!((y[2] - 17.0).abs() < 1e-5);
    }

    #[test]
    fn test_axpy() {
        let x = vec![1.0, 2.0, 3.0];
        let mut y = vec![10.0, 20.0, 30.0];

        axpy(2.0, &x, &mut y).unwrap();

        assert!((y[0] - 12.0).abs() < 1e-5);
        assert!((y[1] - 24.0).abs() < 1e-5);
        assert!((y[2] - 36.0).abs() < 1e-5);
    }

    #[test]
    fn test_lm_head_matmul_shape() {
        // Simulate LM head: hidden_states [1, hidden] @ lm_head.T [hidden, vocab] -> logits [1, vocab]
        let hidden_size = 64; // Small for test
        let vocab_size = 128;

        let hidden_states: Vec<f32> = (0..hidden_size).map(|i| i as f32 * 0.01).collect();
        let lm_head: Vec<f32> = (0..vocab_size * hidden_size).map(|i| (i % 100) as f32 * 0.001).collect();

        // logits = hidden_states @ lm_head.T
        // But lm_head is [vocab, hidden], so we need hidden @ lm_head^T
        // Which is: [1, hidden] @ [hidden, vocab] = [1, vocab]
        // Since lm_head is stored as [vocab, hidden], we transpose it

        // For row-major: C[i,j] = sum_k A[i,k] * B[k,j]
        // If B is stored transposed, we use CblasTrans
        // But simpler: reshape conceptually

        // Actually, for hidden @ lm_head^T with lm_head as [vocab, hidden]:
        // result[v] = sum_h hidden[h] * lm_head[v, h]
        // This is equivalent to: [hidden] @ [hidden, vocab] where the second matrix
        // is the transpose of lm_head

        // Use matvec with transposed interpretation
        let logits = matvec_accelerate(&lm_head, &hidden_states, vocab_size, hidden_size).unwrap();

        assert_eq!(logits.len(), vocab_size);
        // Just verify it doesn't crash and produces reasonable output
        assert!(logits.iter().all(|&v| v.is_finite()));
    }
}
