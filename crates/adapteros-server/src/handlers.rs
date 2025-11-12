// Wrap incomplete AOS 2.0 streaming mocks
#[cfg(feature = "aos2-staging")]
/* TODO: Isolated to staging branch aos2-format; implement full migration */
let mock_stream = async_stream::stream! {
    // Mock events for AOS 2.0 format
    yield Ok(Event::default());
};

#[cfg(not(feature = "aos2-staging"))]
{
    return Err(AosError::Config("AOS 2.0 features disabled; enable staging flag".to_string()));
}

// Extract common stub to services/aos2_stubs::create_mock_stream if duplicated
