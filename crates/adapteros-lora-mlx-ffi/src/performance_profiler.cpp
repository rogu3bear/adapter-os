// Performance Profiling Infrastructure Implementation
// Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.

#include "performance_profiler.h"
#include <sstream>
#include <iomanip>
#include <cstdio>

namespace adapteros {
namespace mlx {
namespace profiler {

// Global performance counters instance
PerformanceCounters g_perf_counters;

// Global profiling enable flag
static std::atomic<bool> g_profiling_enabled{true};

std::string format_counter_json(const char* name, const PerformanceCounter& counter) {
    uint64_t count = counter.count();
    double avg_us = counter.avg_time_us();
    double min_us = counter.min_time_us();
    double max_us = counter.max_time_us();
    double total_ms = counter.total_time_ms();

    std::ostringstream oss;
    oss << std::fixed << std::setprecision(2);
    oss << "  \"" << name << "\": {"
        << "\"count\": " << count << ", "
        << "\"avg_us\": " << avg_us << ", "
        << "\"min_us\": " << min_us << ", "
        << "\"max_us\": " << max_us << ", "
        << "\"total_ms\": " << total_ms
        << "}";

    return oss.str();
}

std::string get_performance_stats_json() {
    std::lock_guard<std::mutex> lock(g_perf_counters.counters_mutex);

    std::ostringstream oss;
    oss << "{\n";
    oss << format_counter_json("matmul", g_perf_counters.matmul) << ",\n";
    oss << format_counter_json("add", g_perf_counters.add) << ",\n";
    oss << format_counter_json("subtract", g_perf_counters.subtract) << ",\n";
    oss << format_counter_json("multiply", g_perf_counters.multiply) << ",\n";
    oss << format_counter_json("divide", g_perf_counters.divide) << ",\n";
    oss << format_counter_json("attention", g_perf_counters.attention) << ",\n";
    oss << format_counter_json("lora_forward", g_perf_counters.lora_forward) << ",\n";
    oss << format_counter_json("multi_lora_forward", g_perf_counters.multi_lora_forward) << ",\n";
    oss << format_counter_json("model_forward", g_perf_counters.model_forward) << ",\n";
    oss << format_counter_json("array_creation", g_perf_counters.array_creation) << ",\n";
    oss << format_counter_json("memory_transfer", g_perf_counters.memory_transfer) << ",\n";
    oss << format_counter_json("eval", g_perf_counters.eval) << ",\n";
    oss << format_counter_json("softmax", g_perf_counters.softmax) << ",\n";
    oss << format_counter_json("activation", g_perf_counters.activation) << "\n";
    oss << "}";

    return oss.str();
}

} // namespace profiler
} // namespace mlx
} // namespace adapteros

// C API implementation
extern "C" {

const char* mlx_get_performance_stats(void) {
    using namespace adapteros::mlx::profiler;

    static thread_local std::string stats_json;
    stats_json = get_performance_stats_json();
    return stats_json.c_str();
}

void mlx_reset_performance_counters(void) {
    using namespace adapteros::mlx::profiler;
    g_perf_counters.reset_all();
}

void mlx_set_profiling_enabled(bool enabled) {
    using namespace adapteros::mlx::profiler;
    // Note: This would be used to conditionally enable ScopedTimer creation
    // For now, we always profile to collect data
    (void)enabled; // Unused in current implementation
}

} // extern "C"
