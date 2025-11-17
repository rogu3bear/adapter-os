use adapteros_core::identity::IdentityEnvelope;
use tracing::{event, Level};

macro_rules! log_with_identity {
    ($envelope:expr, $lvl:expr, $($arg:tt)*) => {
        {
            let identity = $envelope;
            event!($lvl, 
                tenant_id = identity.tenant_id.as_str(),
                domain = identity.domain.as_str(),
                purpose = identity.purpose.as_str(),
                revision = identity.revision.as_str(),
                $($arg)*
            );
        }
    };
}

pub(crate) use log_with_identity;
