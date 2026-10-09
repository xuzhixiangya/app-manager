//! Shared reqwest setup.
//!
//! The updater plugin compiles reqwest with `rustls-no-provider`. Building a
//! client then panics unless a process-wide crypto provider is already
//! installed. The updater used to install ring while checking for a manager
//! update. Company builds no longer run that check, so an async command such
//! as "load models" panicked and the button stayed on "loading" forever.

pub fn builder() -> reqwest::ClientBuilder {
    // A provider may already be installed. That error is the success case.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_client_without_the_updater_installing_tls() {
        assert!(builder().build().is_ok());
    }
}
