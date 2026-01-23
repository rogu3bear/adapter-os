// Clean code file without TODOs or unimplemented macros
//
// This file should be detected as having no obvious issues.

/// A simple calculator module
pub mod calculator {
    /// Adds two numbers
    pub fn add(a: i32, b: i32) -> i32 {
        a + b
    }

    /// Subtracts b from a
    pub fn subtract(a: i32, b: i32) -> i32 {
        a - b
    }

    /// Multiplies two numbers
    pub fn multiply(a: i32, b: i32) -> i32 {
        a * b
    }

    /// Divides a by b, returns None if b is zero
    pub fn divide(a: i32, b: i32) -> Option<i32> {
        if b == 0 {
            None
        } else {
            Some(a / b)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::calculator::*;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 3), 5);
    }

    #[test]
    fn test_divide_by_zero() {
        assert!(divide(10, 0).is_none());
    }
}
