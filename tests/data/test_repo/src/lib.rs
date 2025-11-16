/// Add two numbers together
///
/// This function takes two integers and returns their sum.
/// It's a simple arithmetic operation.
///
/// # Examples
/// ```
/// let result = add(2, 3);
/// assert_eq!(result, 5);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Multiply two numbers
///
/// Returns the product of two integers.
/// Uses the standard multiplication operator.
///
/// # Examples
/// ```
/// let result = multiply(4, 5);
/// assert_eq!(result, 20);
/// ```
pub fn multiply(x: i32, y: i32) -> i32 {
    x * y
}

/// Calculate the square of a number
///
/// Computes n^2 for the given integer.
pub fn square(n: i32) -> i32 {
    n * n
}

/// A utility struct for basic math operations
pub struct Calculator {
    /// The current value
    value: i32,
}

impl Calculator {
    /// Create a new calculator with initial value
    pub fn new(initial: i32) -> Self {
        Self { value: initial }
    }

    /// Add a value to the calculator
    pub fn add(&mut self, n: i32) {
        self.value += n;
    }

    /// Get the current value
    pub fn get(&self) -> i32 {
        self.value
    }
}
