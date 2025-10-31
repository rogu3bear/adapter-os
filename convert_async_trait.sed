# Sed script to convert async fn trait methods to impl Future pattern
# Usage: sed -f convert_async_trait.sed input.rs > output.rs

# Convert async fn to fn with impl Future
/^    async fn /{
    s/async fn /fn /
    s/ -> Result</ -> impl std::future::Future<Output = Result</
    s/>$/>> + Send {/
    N
    s/{\n/{\n        async {\n/
    :loop
    n
    /    }$/{
        s/    }$/        }\n    }/
        b end
    }
    s/^/        /
    b loop
    :end
}
