pub mod audit;
pub mod inventory;
pub mod sync;

use archive_it_client::Config;

/// Attach the optional allow-list header to a client config.
fn with_header(mut cfg: Config, header: Option<&(String, String)>) -> Config {
    if let Some((name, value)) = header {
        cfg.headers.push((name.clone(), value.clone()));
    }
    cfg
}
