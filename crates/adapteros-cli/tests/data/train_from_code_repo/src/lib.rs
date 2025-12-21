//! Sample library used for train-from-code integration tests.

pub mod widgets {
    /// Generates a human friendly description for a widget size.
    ///
    /// This is intentionally verbose so the ingestion pipeline has
    /// something substantial to extract from the code graph docstrings.
    pub fn describe_widget(size: u32) -> String {
        format!("Widget size: {} units", size)
    }

    /// Example of a documented helper that references arguments.
    pub fn compute_capacity(width: u32, height: u32) -> u32 {
        width * height
    }

    pub fn undocumented_helper(token: &str) -> String {
        format!("helper:{}", token)
    }
}
