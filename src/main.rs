#[allow(unused_imports)]
use ahoy_dtu_stats::{entrypoint, ErrorKind};

#[tokio::main]
#[cfg(not(test))]
async fn main() -> Result<(), ErrorKind> {
    entrypoint().await
}
