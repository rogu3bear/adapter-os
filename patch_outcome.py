import re

with open('crates/adapteros-core/src/recovery/outcome.rs', 'r') as f:
    c = f.read()

c = c.replace('RecoveryOutcome<T>', 'RecoveryOutcome<T, E = AosError>')
c = c.replace('impl<T, E = AosError>', 'impl<T, E>')  # Fix previous replace
c = c.replace('impl<T> RecoveryOutcome<T, E = AosError>', 'impl<T, E> RecoveryOutcome<T, E>')
c = c.replace('pub result: Result<T, RecoveryError>,', 'pub result: Result<T, RecoveryError<E>>,')
c = c.replace('pub fn failure(error: RecoveryError, stats: RecoveryStats) -> Self', 'pub fn failure(error: RecoveryError<E>, stats: RecoveryStats) -> Self')
c = c.replace('-> RecoveryOutcome<U>', '-> RecoveryOutcome<U, E>')

c = c.replace('pub enum RecoveryError {', 'pub enum RecoveryError<E = AosError> {')
c = c.replace('source: AosError,', 'source: E,')
c = c.replace('impl RecoveryError {', 'impl<E> RecoveryError<E> {')
c = c.replace('-> Option<&AosError>', '-> Option<&E>')

c = c.replace('impl From<RecoveryError> for AosError', 'impl From<RecoveryError<AosError>> for AosError')
c = c.replace('fn from(err: RecoveryError) -> Self', 'fn from(err: RecoveryError<AosError>) -> Self')

c = c.replace('RecoveryOutcome<i32>', 'RecoveryOutcome<i32, AosError>')

with open('crates/adapteros-core/src/recovery/outcome.rs', 'w') as f:
    f.write(c)
