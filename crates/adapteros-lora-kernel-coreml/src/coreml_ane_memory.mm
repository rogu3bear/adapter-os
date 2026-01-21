//! ANE memory querying and model tracking implementation
//!
//! Provides functions to query Apple Neural Engine memory usage statistics
//! and track model load/unload events for accurate memory accounting.

#import "coreml_ffi.h"
#import <Foundation/Foundation.h>
#import <IOKit/IOKitLib.h>
#import <os/lock.h>

// ========== Model Memory Tracker ==========

// Thread-safe dictionary to track loaded models and their memory footprints
static NSMutableDictionary<NSString *, NSNumber *> *g_model_registry = nil;
static os_unfair_lock g_registry_lock = OS_UNFAIR_LOCK_INIT;
static uint64_t g_total_allocated = 0;
static uint64_t g_peak_allocated = 0;

static void ensure_registry_initialized() {
  if (g_model_registry == nil) {
    os_unfair_lock_lock(&g_registry_lock);
    if (g_model_registry == nil) {
      g_model_registry = [[NSMutableDictionary alloc] init];
    }
    os_unfair_lock_unlock(&g_registry_lock);
  }
}

extern "C" void swift_coreml_record_model_load(const char *model_id,
                                               uint64_t bytes) {
  if (!model_id)
    return;

  ensure_registry_initialized();

  NSString *key = [NSString stringWithUTF8String:model_id];
  NSNumber *value = [NSNumber numberWithUnsignedLongLong:bytes];

  os_unfair_lock_lock(&g_registry_lock);

  // If model already exists, subtract old value first
  NSNumber *existing = g_model_registry[key];
  if (existing) {
    g_total_allocated -= [existing unsignedLongLongValue];
  }

  g_model_registry[key] = value;
  g_total_allocated += bytes;

  // Track peak
  if (g_total_allocated > g_peak_allocated) {
    g_peak_allocated = g_total_allocated;
  }

  os_unfair_lock_unlock(&g_registry_lock);
}

extern "C" void swift_coreml_record_model_unload(const char *model_id) {
  if (!model_id)
    return;

  ensure_registry_initialized();

  NSString *key = [NSString stringWithUTF8String:model_id];

  os_unfair_lock_lock(&g_registry_lock);

  NSNumber *existing = g_model_registry[key];
  if (existing) {
    g_total_allocated -= [existing unsignedLongLongValue];
    [g_model_registry removeObjectForKey:key];
  }

  os_unfair_lock_unlock(&g_registry_lock);
}

extern "C" int32_t swift_coreml_loaded_model_count() {
  ensure_registry_initialized();

  os_unfair_lock_lock(&g_registry_lock);
  int32_t count = (int32_t)[g_model_registry count];
  os_unfair_lock_unlock(&g_registry_lock);

  return count;
}

extern "C" void swift_coreml_reset_memory_tracker() {
  os_unfair_lock_lock(&g_registry_lock);

  if (g_model_registry) {
    [g_model_registry removeAllObjects];
  }
  g_total_allocated = 0;
  g_peak_allocated = 0;

  os_unfair_lock_unlock(&g_registry_lock);
}

// ========== IOKit ANE Service Discovery ==========

static const char *kAneServiceNames[] = {"AppleH16CamIn",     // M3, M4 era
                                         "AppleH13CamIn",     // M1, M2 era
                                         "AppleNeuralEngine", // Generic name
                                         NULL};

/// Query ANE memory via IOKit hardware service
static bool query_ane_via_iokit(AneMemoryInfo *info) {
  if (!info) {
    return false;
  }

  io_iterator_t iterator = 0;
  io_service_t service = 0;
  kern_return_t kr;
  bool found = false;

  // Try each known ANE service name
  for (int i = 0; kAneServiceNames[i] != NULL; i++) {
    kr = IOServiceGetMatchingServices(
        kIOMainPortDefault, IOServiceMatching(kAneServiceNames[i]), &iterator);

    if (kr == KERN_SUCCESS && iterator != 0) {
      service = IOIteratorNext(iterator);
      if (service != 0) {
        found = true;
        break;
      }
      IOObjectRelease(iterator);
      iterator = 0;
    }
  }

  if (!found || service == 0) {
    if (iterator != 0) {
      IOObjectRelease(iterator);
    }
    return false;
  }

  // ANE service found - use our tracked memory as the authoritative source
  info->available = true;

  os_unfair_lock_lock(&g_registry_lock);
  info->allocated_bytes = g_total_allocated;
  info->used_bytes = g_total_allocated; // All allocated is in use
  info->peak_bytes = g_peak_allocated;
  os_unfair_lock_unlock(&g_registry_lock);

  // Try to read system-level properties (may not be exposed)
  CFTypeRef throttled_ref = IORegistryEntryCreateCFProperty(
      service, CFSTR("thermal-throttled"), kCFAllocatorDefault, 0);

  if (throttled_ref && CFGetTypeID(throttled_ref) == CFBooleanGetTypeID()) {
    info->throttled = CFBooleanGetValue((CFBooleanRef)throttled_ref);
  }

  if (throttled_ref)
    CFRelease(throttled_ref);

  IOObjectRelease(service);
  IOObjectRelease(iterator);

  return true;
}

// ========== Public FFI Functions ==========

extern "C" int32_t swift_coreml_get_ane_memory_info(
    bool *out_available, uint64_t *out_allocated_bytes,
    uint64_t *out_used_bytes, uint64_t *out_cached_bytes,
    uint64_t *out_peak_bytes, bool *out_throttled) {
  if (!out_available || !out_allocated_bytes || !out_used_bytes ||
      !out_cached_bytes || !out_peak_bytes || !out_throttled) {
    return 0;
  }

  ensure_registry_initialized();

  AneMemoryInfo info = {0};

  bool result = query_ane_via_iokit(&info);

  *out_available = result && info.available;
  *out_allocated_bytes = info.allocated_bytes;
  *out_used_bytes = info.used_bytes;
  *out_cached_bytes = info.cached_bytes;
  *out_peak_bytes = info.peak_bytes;
  *out_throttled = info.throttled;

  return result ? 1 : 0;
}

extern "C" void coreml_debug_dump_ane_registry(void) {
  io_iterator_t iterator = 0;
  io_service_t service = 0;
  kern_return_t kr;

  printf("=== ANE Registry Probe ===\n");

  for (int i = 0; kAneServiceNames[i] != NULL; i++) {
    printf("Checking service: %s\n", kAneServiceNames[i]);
    kr = IOServiceGetMatchingServices(
        kIOMainPortDefault, IOServiceMatching(kAneServiceNames[i]), &iterator);

    if (kr == KERN_SUCCESS && iterator != 0) {
      while ((service = IOIteratorNext(iterator)) != 0) {
        printf("  Found! Dumping properties:\n");

        CFMutableDictionaryRef properties = NULL;
        kr = IORegistryEntryCreateCFProperties(service, &properties,
                                               kCFAllocatorDefault, 0);
        if (kr == KERN_SUCCESS && properties != NULL) {
          // Convert to string for display
          NSString *desc = [(__bridge NSDictionary *)properties description];
          printf("%s\n", [desc UTF8String]);
          CFRelease(properties);
        }
        IOObjectRelease(service);
      }
      IOObjectRelease(iterator);
    } else {
      printf("  Not found\n");
    }
  }

  // Also dump our tracker state
  printf("\n=== Memory Tracker State ===\n");
  ensure_registry_initialized();
  os_unfair_lock_lock(&g_registry_lock);
  printf("Loaded models: %lu\n", (unsigned long)[g_model_registry count]);
  printf("Total allocated: %llu bytes\n", g_total_allocated);
  printf("Peak allocated: %llu bytes\n", g_peak_allocated);
  for (NSString *key in g_model_registry) {
    printf("  %s: %llu bytes\n", [key UTF8String],
           [g_model_registry[key] unsignedLongLongValue]);
  }
  os_unfair_lock_unlock(&g_registry_lock);
}

extern "C" bool coreml_reset_ane_peak(void) {
  os_unfair_lock_lock(&g_registry_lock);
  g_peak_allocated = g_total_allocated;
  os_unfair_lock_unlock(&g_registry_lock);
  return true;
}
