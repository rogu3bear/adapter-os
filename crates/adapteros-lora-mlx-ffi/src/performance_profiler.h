// Performance Profiling Infrastructure for MLX Backend
// Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.

#ifndef ADAPTEROS_MLX_PERFORMANCE_PROFILER_H
#define ADAPTEROS_MLX_PERFORMANCE_PROFILER_H

#include <atomic>
#include <chrono>
#include <mutex>
#include <cstdint>
#include <string>

namespace adapteros {
namespace mlx {
namespace profiler {

/// Performance counter for operation timing
struct PerformanceCounter {
    std::atomic<uint64_t> call_count{0};
    std::atomic<uint64_t> total_time_ns{0};
    std::atomic<uint64_t> min_time_ns{UINT64_MAX};
    std::atomic<uint64_t> max_time_ns{0};

    void record(uint64_t duration_ns) {
        call_count.fetch_add(1, std::memory_order_relaxed);
        total_time_ns.fetch_add(duration_ns, std::memory_order_relaxed);

        // Update min (lock-free best-effort)
        uint64_t current_min = min_time_ns.load(std::memory_order_relaxed);
        while (duration_ns < current_min &&
               !min_time_ns.compare_exchange_weak(current_min, duration_ns,
                   std::memory_order_relaxed)) {}

        // Update max (lock-free best-effort)
        uint64_t current_max = max_time_ns.load(std::memory_order_relaxed);
        while (duration_ns > current_max &&
               !max_time_ns.compare_exchange_weak(current_max, duration_ns,
                   std::memory_order_relaxed)) {}
    }

    double avg_time_us() const {
        uint64_t count = call_count.load(std::memory_order_relaxed);
        if (count == 0) return 0.0;
        return (total_time_ns.load(std::memory_order_relaxed) / static_cast<double>(count)) / 1000.0;
    }

    uint64_t count() const {
        return call_count.load(std::memory_order_relaxed);
    }

    double total_time_ms() const {
        return total_time_ns.load(std::memory_order_relaxed) / 1000000.0;
    }

    double min_time_us() const {
        uint64_t min = min_time_ns.load(std::memory_order_relaxed);
        return (min != UINT64_MAX) ? (min / 1000.0) : 0.0;
    }

    double max_time_us() const {
        return max_time_ns.load(std::memory_order_relaxed) / 1000.0;
    }

    void reset() {
        call_count.store(0, std::memory_order_relaxed);
        total_time_ns.store(0, std::memory_order_relaxed);
        min_time_ns.store(UINT64_MAX, std::memory_order_relaxed);
        max_time_ns.store(0, std::memory_order_relaxed);
    }
};

/// Global performance counters for each operation type
struct PerformanceCounters {
    PerformanceCounter matmul;
    PerformanceCounter add;
    PerformanceCounter subtract;
    PerformanceCounter multiply;
    PerformanceCounter divide;
    PerformanceCounter attention;
    PerformanceCounter lora_forward;
    PerformanceCounter multi_lora_forward;
    PerformanceCounter model_forward;
    PerformanceCounter array_creation;
    PerformanceCounter memory_transfer;
    PerformanceCounter eval;
    PerformanceCounter softmax;
    PerformanceCounter activation;
    std::mutex counters_mutex;

    void reset_all() {
        std::lock_guard<std::mutex> lock(counters_mutex);
        matmul.reset();
        add.reset();
        subtract.reset();
        multiply.reset();
        divide.reset();
        attention.reset();
        lora_forward.reset();
        multi_lora_forward.reset();
        model_forward.reset();
        array_creation.reset();
        memory_transfer.reset();
        eval.reset();
        softmax.reset();
        activation.reset();
    }
};

/// RAII timer for automatic operation timing
class ScopedTimer {
    std::chrono::high_resolution_clock::time_point start_;
    PerformanceCounter& counter_;

public:
    explicit ScopedTimer(PerformanceCounter& counter)
        : start_(std::chrono::high_resolution_clock::now()), counter_(counter) {}

    ~ScopedTimer() {
        auto end = std::chrono::high_resolution_clock::now();
        auto duration = std::chrono::duration_cast<std::chrono::nanoseconds>(end - start_);
        counter_.record(duration.count());
    }

    // Non-copyable
    ScopedTimer(const ScopedTimer&) = delete;
    ScopedTimer& operator=(const ScopedTimer&) = delete;
};

/// Global performance counters instance
extern PerformanceCounters g_perf_counters;

/// Format performance counter as JSON object
std::string format_counter_json(const char* name, const PerformanceCounter& counter);

/// Get all performance statistics as JSON
std::string get_performance_stats_json();

} // namespace profiler
} // namespace mlx
} // namespace adapteros

// C API for Rust FFI
extern "C" {
    /// Get performance statistics as JSON string
    const char* mlx_get_performance_stats(void);

    /// Reset all performance counters
    void mlx_reset_performance_counters(void);

    /// Enable/disable performance profiling
    void mlx_set_profiling_enabled(bool enabled);
}

#endif // ADAPTEROS_MLX_PERFORMANCE_PROFILER_H
