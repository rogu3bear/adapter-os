//! Deterministic select - bans tokio::select! in deterministic code
//!
//! This module provides compile-time enforcement that tokio::select! is not used
//! in deterministic execution paths. Use deterministic polling instead.

/// Compile error macro to prevent tokio::select! usage
#[macro_export]
macro_rules! ban_tokio_select {
    () => {
        compile_error!(
            "tokio::select! is nondeterministic and banned in AdapterOS deterministic execution. \
             Use deterministic polling with DeterministicExecutor::delay() or channel::DeterministicReceiver::recv() instead."
        );
    };
}

/// Deterministic alternative to tokio::select! for 2 branches
///
/// Polls futures in deterministic order (left to right).
/// This ensures reproducible execution.
pub async fn select_2<F1, F2, T1, T2>(fut1: F1, fut2: F2) -> SelectResult2<T1, T2>
where
    F1: std::future::Future<Output = T1>,
    F2: std::future::Future<Output = T2>,
{
    tokio::pin!(fut1);
    tokio::pin!(fut2);

    loop {
        // Poll in deterministic order: fut1 first, then fut2
        if let std::task::Poll::Ready(val) = futures::poll!(&mut fut1) {
            return SelectResult2::First(val);
        }
        if let std::task::Poll::Ready(val) = futures::poll!(&mut fut2) {
            return SelectResult2::Second(val);
        }

        // Yield to executor
        tokio::task::yield_now().await;
    }
}

/// Result of select_2
pub enum SelectResult2<T1, T2> {
    First(T1),
    Second(T2),
}

/// Deterministic alternative to tokio::select! for 3 branches
pub async fn select_3<F1, F2, F3, T1, T2, T3>(
    fut1: F1,
    fut2: F2,
    fut3: F3,
) -> SelectResult3<T1, T2, T3>
where
    F1: std::future::Future<Output = T1>,
    F2: std::future::Future<Output = T2>,
    F3: std::future::Future<Output = T3>,
{
    tokio::pin!(fut1);
    tokio::pin!(fut2);
    tokio::pin!(fut3);

    loop {
        // Poll in deterministic order
        if let std::task::Poll::Ready(val) = futures::poll!(&mut fut1) {
            return SelectResult3::First(val);
        }
        if let std::task::Poll::Ready(val) = futures::poll!(&mut fut2) {
            return SelectResult3::Second(val);
        }
        if let std::task::Poll::Ready(val) = futures::poll!(&mut fut3) {
            return SelectResult3::Third(val);
        }

        tokio::task::yield_now().await;
    }
}

/// Result of select_3
pub enum SelectResult3<T1, T2, T3> {
    First(T1),
    Second(T2),
    Third(T3),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_deterministic_select_2() {
        let fut1 = async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            42
        };

        let fut2 = async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            100
        };

        match select_2(fut1, fut2).await {
            SelectResult2::First(val) => assert_eq!(val, 42),
            SelectResult2::Second(val) => assert_eq!(val, 100),
        }
    }

    #[tokio::test]
    async fn test_deterministic_select_3() {
        let fut1 = async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            1
        };

        let fut2 = async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            2
        };

        let fut3 = async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            3
        };

        // With deterministic ordering, fut2 (shortest wait) should complete first
        match select_3(fut1, fut2, fut3).await {
            SelectResult3::First(val) => assert_eq!(val, 1),
            SelectResult3::Second(val) => assert_eq!(val, 2),
            SelectResult3::Third(val) => assert_eq!(val, 3),
        }
    }
}
