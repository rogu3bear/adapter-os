//! Enhanced error handling utilities for MLX C++ wrapper
//! Provides structured error capture, GPU OOM detection, and stack trace support

#ifndef MLX_ERROR_HANDLING_HPP
#define MLX_ERROR_HANDLING_HPP

#include <string>
#include <exception>
#include <stdexcept>
#include <sstream>
#include <vector>
#include <cstring>

#ifdef MLX_USE_REAL
#include <mlx/mlx.h>
#endif

namespace mlx_error {

/// Error categories for structured error handling
enum class ErrorCategory {
    GPU_OOM,              // GPU out of memory
    CPU_OOM,              // CPU out of memory
    TENSOR_SHAPE,         // Shape mismatch
    TENSOR_DTYPE,         // Data type mismatch
    NULL_POINTER,         // Null pointer access
    INVALID_ARGUMENT,     // Invalid function argument
    IO_ERROR,             // File I/O error
    PARSE_ERROR,          // Parsing/deserialization error
    MLX_RUNTIME,          // MLX runtime error
    UNKNOWN               // Unknown/uncategorized error
};

/// Convert error category to string
inline const char* category_to_string(ErrorCategory cat) {
    switch (cat) {
        case ErrorCategory::GPU_OOM: return "GPU_OOM";
        case ErrorCategory::CPU_OOM: return "CPU_OOM";
        case ErrorCategory::TENSOR_SHAPE: return "TENSOR_SHAPE";
        case ErrorCategory::TENSOR_DTYPE: return "TENSOR_DTYPE";
        case ErrorCategory::NULL_POINTER: return "NULL_POINTER";
        case ErrorCategory::INVALID_ARGUMENT: return "INVALID_ARGUMENT";
        case ErrorCategory::IO_ERROR: return "IO_ERROR";
        case ErrorCategory::PARSE_ERROR: return "PARSE_ERROR";
        case ErrorCategory::MLX_RUNTIME: return "MLX_RUNTIME";
        case ErrorCategory::UNKNOWN: return "UNKNOWN";
        default: return "UNRECOGNIZED";
    }
}

/// Detect error category from exception message
inline ErrorCategory detect_category(const std::string& message) {
    std::string lower_msg = message;
    for (auto& c : lower_msg) c = std::tolower(c);

    if (lower_msg.find("out of memory") != std::string::npos ||
        lower_msg.find("oom") != std::string::npos ||
        lower_msg.find("allocation failed") != std::string::npos) {
        if (lower_msg.find("gpu") != std::string::npos ||
            lower_msg.find("metal") != std::string::npos ||
            lower_msg.find("device") != std::string::npos) {
            return ErrorCategory::GPU_OOM;
        }
        return ErrorCategory::CPU_OOM;
    }

    if (lower_msg.find("shape") != std::string::npos ||
        lower_msg.find("dimension") != std::string::npos ||
        lower_msg.find("size mismatch") != std::string::npos) {
        return ErrorCategory::TENSOR_SHAPE;
    }

    if (lower_msg.find("dtype") != std::string::npos ||
        lower_msg.find("type mismatch") != std::string::npos ||
        lower_msg.find("invalid type") != std::string::npos) {
        return ErrorCategory::TENSOR_DTYPE;
    }

    if (lower_msg.find("null") != std::string::npos ||
        lower_msg.find("nullptr") != std::string::npos) {
        return ErrorCategory::NULL_POINTER;
    }

    if (lower_msg.find("invalid argument") != std::string::npos ||
        lower_msg.find("invalid parameter") != std::string::npos) {
        return ErrorCategory::INVALID_ARGUMENT;
    }

    if (lower_msg.find("file") != std::string::npos ||
        lower_msg.find("i/o") != std::string::npos ||
        lower_msg.find("not found") != std::string::npos) {
        return ErrorCategory::IO_ERROR;
    }

    if (lower_msg.find("parse") != std::string::npos ||
        lower_msg.find("deserialize") != std::string::npos ||
        lower_msg.find("invalid format") != std::string::npos) {
        return ErrorCategory::PARSE_ERROR;
    }

    return ErrorCategory::MLX_RUNTIME;
}

/// Structured error information
struct ErrorInfo {
    ErrorCategory category;
    std::string function;
    std::string message;
    std::string context;
    bool recoverable;

    std::string format() const {
        std::ostringstream oss;
        oss << "[" << category_to_string(category) << "] "
            << function << ": " << message;
        if (!context.empty()) {
            oss << " (context: " << context << ")";
        }
        return oss.str();
    }
};

/// Simple stack trace capture (platform-specific)
class StackTrace {
public:
    static std::string capture() {
        // Basic stack trace - would need platform-specific code for real traces
        // On macOS/Linux: could use backtrace() and backtrace_symbols()
        // For MVP: just return placeholder
        return "[Stack trace not available in this build]";
    }
};

/// Error handler with detailed context
class ErrorHandler {
private:
    static thread_local ErrorInfo last_error_;

public:
    /// Set last error
    static void set_error(ErrorInfo error) {
        last_error_ = std::move(error);
    }

    /// Get last error
    static const ErrorInfo& last_error() {
        return last_error_;
    }

    /// Clear last error
    static void clear() {
        last_error_ = ErrorInfo{
            ErrorCategory::UNKNOWN,
            "",
            "",
            "",
            false
        };
    }

    /// Create error from exception
    static ErrorInfo from_exception(
        const std::exception& e,
        const char* function,
        const char* context = ""
    ) {
        std::string message = e.what();
        ErrorCategory category = detect_category(message);

        bool recoverable = (
            category == ErrorCategory::GPU_OOM ||
            category == ErrorCategory::CPU_OOM ||
            category == ErrorCategory::IO_ERROR ||
            category == ErrorCategory::MLX_RUNTIME
        );

        return ErrorInfo{
            category,
            function,
            message,
            context,
            recoverable
        };
    }
};

// Thread-local storage for last error
thread_local ErrorInfo ErrorHandler::last_error_ = {
    ErrorCategory::UNKNOWN,
    "",
    "",
    "",
    false
};

/// RAII error context manager
class ErrorContext {
private:
    std::string function_;
    std::string context_;

public:
    ErrorContext(const char* function, const char* context = "")
        : function_(function), context_(context) {}

    /// Execute operation with error handling
    template<typename Func>
    auto execute(Func&& func) noexcept -> decltype(func()) {
        try {
            return func();
        } catch (const std::bad_alloc& e) {
            ErrorHandler::set_error(ErrorInfo{
                ErrorCategory::CPU_OOM,
                function_,
                std::string("Memory allocation failed: ") + e.what(),
                context_,
                true  // OOM is recoverable with cleanup
            });
            return nullptr;
        } catch (const std::exception& e) {
            ErrorHandler::set_error(
                ErrorHandler::from_exception(e, function_.c_str(), context_.c_str())
            );
            return nullptr;
        } catch (...) {
            ErrorHandler::set_error(ErrorInfo{
                ErrorCategory::UNKNOWN,
                function_,
                "Unknown exception caught",
                context_,
                false
            });
            return nullptr;
        }
    }

    /// Execute void operation with error handling
    template<typename Func>
    bool execute_void(Func&& func) noexcept {
        try {
            func();
            return true;
        } catch (const std::bad_alloc& e) {
            ErrorHandler::set_error(ErrorInfo{
                ErrorCategory::CPU_OOM,
                function_,
                std::string("Memory allocation failed: ") + e.what(),
                context_,
                true
            });
            return false;
        } catch (const std::exception& e) {
            ErrorHandler::set_error(
                ErrorHandler::from_exception(e, function_.c_str(), context_.c_str())
            );
            return false;
        } catch (...) {
            ErrorHandler::set_error(ErrorInfo{
                ErrorCategory::UNKNOWN,
                function_,
                "Unknown exception caught",
                context_,
                false
            });
            return false;
        }
    }
};

/// Macro for safe execution with error handling
#define MLX_SAFE_CALL(func_name, operation) \
    mlx_error::ErrorContext(__func__, func_name).execute([&]() { return operation; })

#define MLX_SAFE_CALL_VOID(func_name, operation) \
    mlx_error::ErrorContext(__func__, func_name).execute_void([&]() { operation; })

/// Format error message with details
inline std::string format_error(
    const char* function,
    const char* operation,
    const std::string& details
) {
    std::ostringstream oss;
    oss << function << " failed during '" << operation << "': " << details;
    return oss.str();
}

/// Check memory availability before allocation
inline bool check_memory_available(size_t required_bytes, const char* operation) {
    // This would need platform-specific implementation
    // For MVP: just log the requirement
    #ifdef MLX_VERBOSE_MEMORY
    std::cerr << "[MEMORY] " << operation << " requires "
              << (required_bytes / 1024.0 / 1024.0) << " MB" << std::endl;
    #endif
    return true;  // Optimistically assume available
}

} // namespace mlx_error

#endif // MLX_ERROR_HANDLING_HPP
