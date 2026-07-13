mod ipc;
mod singleton_lease;
mod singleton_lease_error;

pub(crate) use singleton_lease::SingletonLease;
pub(crate) use singleton_lease_error::SingletonLeaseError;

#[cfg(test)]
mod tests;
