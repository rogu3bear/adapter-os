//! CoreML backend implementation with power management
//!
//! Copyright: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

#import <Foundation/Foundation.h>
#import <CoreML/CoreML.h>
#import <IOKit/ps/IOPowerSources.h>
#import <IOKit/ps/IOPSKeys.h>

#if TARGET_OS_IOS
#import <UIKit/UIKit.h>
#else
#import <Foundation/NSProcessInfo.h>
#endif

// Detect ANE availability (heuristic: check device capabilities)
static BOOL detect_ane_availability() {
    // ANE available on M1+ devices (Apple Silicon)
    if (@available(macOS 13.0, iOS 16.0, *)) {
        return YES; // Assume ANE available on macOS 13+ / iOS 16+
    }
    return NO;
}

extern "C" void* coreml_load_model(
    const char* model_path,
    char* error_buffer,
    size_t error_size,
    int32_t* ane_available
) {
    @autoreleasepool {
        NSURL* url = [NSURL fileURLWithPath:@(model_path)];
        MLModelConfiguration* config = [[MLModelConfiguration alloc] init];
        config.computeUnits = MLComputeUnitsAll;

        NSError* error = nil;
        MLModel* model = [MLModel modelWithContentsOfURL:url
                                           configuration:config
                                                   error:&error];

        if (error) {
            const char* msg = [error.localizedDescription UTF8String];
            strncpy(error_buffer, msg, error_size - 1);
            error_buffer[error_size - 1] = '\0';
            return nullptr;
        }

        *ane_available = detect_ane_availability() ? 1 : 0;
        return (__bridge_retained void*)model;
    }
}

extern "C" void coreml_release_model(void* model_ptr) {
    if (model_ptr) {
        CFRelease(model_ptr);
    }
}

typedef struct {
    int32_t success;
    int32_t used_ane;
} CoreMLPredictionResult;

extern "C" CoreMLPredictionResult coreml_predict(
    void* model_ptr,
    const uint32_t* input_ids,
    size_t input_len,
    float* output_logits,
    size_t output_size,
    const uint16_t* adapter_indices,
    const int16_t* adapter_gates,
    size_t k
) {
    @autoreleasepool {
        MLModel* model = (__bridge MLModel*)model_ptr;

        NSError* error = nil;
        NSArray<NSNumber*>* shape = @[@(1), @(input_len)];
        MLMultiArray* inputArray = [[MLMultiArray alloc]
            initWithShape:shape
            dataType:MLMultiArrayDataTypeInt32
            error:&error];

        if (error) {
            return (CoreMLPredictionResult){.success = 0, .used_ane = 0};
        }

        for (size_t i = 0; i < input_len; i++) {
            [inputArray setObject:@(input_ids[i]) atIndexedSubscript:i];
        }

        MLDictionaryFeatureProvider* inputProvider = [[MLDictionaryFeatureProvider alloc]
            initWithDictionary:@{@"input_ids": inputArray}
            error:&error];

        if (error) {
            return (CoreMLPredictionResult){.success = 0, .used_ane = 0};
        }

        MLPredictionOptions* options = [[MLPredictionOptions alloc] init];
        id<MLFeatureProvider> output = [model predictionFromFeatures:inputProvider
                                                              options:options
                                                                error:&error];

        if (error) {
            return (CoreMLPredictionResult){.success = 0, .used_ane = 0};
        }

        MLFeatureValue* logitsFeature = [output featureValueForName:@"logits"];
        MLMultiArray* logitsArray = logitsFeature.multiArrayValue;

        size_t copy_len = MIN(output_size, (size_t)logitsArray.count);
        for (size_t i = 0; i < copy_len; i++) {
            output_logits[i] = [logitsArray[i] floatValue];
        }

        int32_t used_ane = detect_ane_availability() ? 1 : 0;
        return (CoreMLPredictionResult){.success = 1, .used_ane = used_ane};
    }
}

extern "C" int32_t coreml_detect_ane() {
    return detect_ane_availability() ? 1 : 0;
}

extern "C" int32_t coreml_ane_core_count() {
    if (@available(macOS 13.0, iOS 16.0, *)) {
        return 16; // M1/M2/M3/M4 have 16 ANE cores
    }
    return 0;
}

extern "C" float coreml_ane_tops() {
    if (@available(macOS 13.0, iOS 16.0, *)) {
        return 15.8f; // M1: 15.8 TOPS, M2/M3/M4: 17.0 TOPS (conservative)
    }
    return 0.0f;
}

extern "C" float get_battery_level() {
    @autoreleasepool {
#if TARGET_OS_IOS
        UIDevice* device = [UIDevice currentDevice];
        device.batteryMonitoringEnabled = YES;
        return device.batteryLevel * 100.0f;
#else
        CFTypeRef powerSourcesInfo = IOPSCopyPowerSourcesInfo();
        if (!powerSourcesInfo) {
            return 100.0f;
        }

        CFArrayRef powerSourcesList = IOPSCopyPowerSourcesList(powerSourcesInfo);
        if (!powerSourcesList) {
            CFRelease(powerSourcesInfo);
            return 100.0f;
        }

        float batteryLevel = 100.0f;
        CFIndex count = CFArrayGetCount(powerSourcesList);

        for (CFIndex i = 0; i < count; i++) {
            CFTypeRef powerSource = CFArrayGetValueAtIndex(powerSourcesList, i);
            CFDictionaryRef description = IOPSGetPowerSourceDescription(powerSourcesInfo, powerSource);

            if (description) {
                CFStringRef type = (CFStringRef)CFDictionaryGetValue(description, CFSTR(kIOPSTypeKey));
                if (type && CFStringCompare(type, CFSTR(kIOPSInternalBatteryType), 0) == kCFCompareEqualTo) {
                    CFNumberRef currentCapacity = (CFNumberRef)CFDictionaryGetValue(description, CFSTR(kIOPSCurrentCapacityKey));
                    CFNumberRef maxCapacity = (CFNumberRef)CFDictionaryGetValue(description, CFSTR(kIOPSMaxCapacityKey));

                    if (currentCapacity && maxCapacity) {
                        int current = 0, max = 0;
                        CFNumberGetValue(currentCapacity, kCFNumberIntType, &current);
                        CFNumberGetValue(maxCapacity, kCFNumberIntType, &max);

                        if (max > 0) {
                            batteryLevel = (float)current / (float)max * 100.0f;
                        }
                    }
                    break;
                }
            }
        }

        CFRelease(powerSourcesList);
        CFRelease(powerSourcesInfo);
        return batteryLevel;
#endif
    }
}

extern "C" int32_t get_is_plugged_in() {
    @autoreleasepool {
#if TARGET_OS_IOS
        UIDevice* device = [UIDevice currentDevice];
        device.batteryMonitoringEnabled = YES;
        UIDeviceBatteryState state = device.batteryState;
        return (state == UIDeviceBatteryStateCharging || state == UIDeviceBatteryStateFull) ? 1 : 0;
#else
        CFTypeRef powerSourcesInfo = IOPSCopyPowerSourcesInfo();
        if (!powerSourcesInfo) {
            return 1;
        }

        CFArrayRef powerSourcesList = IOPSCopyPowerSourcesList(powerSourcesInfo);
        if (!powerSourcesList) {
            CFRelease(powerSourcesInfo);
            return 1;
        }

        int32_t isPluggedIn = 0;
        CFIndex count = CFArrayGetCount(powerSourcesList);

        for (CFIndex i = 0; i < count; i++) {
            CFTypeRef powerSource = CFArrayGetValueAtIndex(powerSourcesList, i);
            CFDictionaryRef description = IOPSGetPowerSourceDescription(powerSourcesInfo, powerSource);

            if (description) {
                CFStringRef powerSourceState = (CFStringRef)CFDictionaryGetValue(description, CFSTR(kIOPSPowerSourceStateKey));
                if (powerSourceState && CFStringCompare(powerSourceState, CFSTR(kIOPSACPowerValue), 0) == kCFCompareEqualTo) {
                    isPluggedIn = 1;
                    break;
                }
            }
        }

        CFRelease(powerSourcesList);
        CFRelease(powerSourcesInfo);
        return isPluggedIn;
#endif
    }
}

extern "C" int32_t get_system_low_power_mode() {
    @autoreleasepool {
#if TARGET_OS_IOS
        if (@available(iOS 9.0, *)) {
            return [[NSProcessInfo processInfo] isLowPowerModeEnabled] ? 1 : 0;
        }
        return 0;
#else
        float batteryLevel = get_battery_level();
        int32_t isPluggedIn = get_is_plugged_in();
        return (!isPluggedIn && batteryLevel < 20.0f) ? 1 : 0;
#endif
    }
}

extern "C" int32_t get_thermal_state() {
    @autoreleasepool {
#if TARGET_OS_IOS
        if (@available(iOS 11.0, *)) {
            NSProcessInfoThermalState state = [[NSProcessInfo processInfo] thermalState];
            switch (state) {
                case NSProcessInfoThermalStateNominal:
                    return 0;
                case NSProcessInfoThermalStateFair:
                    return 1;
                case NSProcessInfoThermalStateSerious:
                    return 2;
                case NSProcessInfoThermalStateCritical:
                    return 3;
                default:
                    return 0;
            }
        }
        return 0;
#else
        if (@available(macOS 10.10.3, *)) {
            NSProcessInfoThermalState state = [[NSProcessInfo processInfo] thermalState];
            switch (state) {
                case NSProcessInfoThermalStateNominal:
                    return 0;
                case NSProcessInfoThermalStateFair:
                    return 1;
                case NSProcessInfoThermalStateSerious:
                    return 2;
                case NSProcessInfoThermalStateCritical:
                    return 3;
                default:
                    return 0;
            }
        }
        return 0;
#endif
    }
}
