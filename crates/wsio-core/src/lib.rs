#[cfg(not(target_pointer_width = "64"))]
compile_error!("wsio-core only supports 64-bit targets");

pub mod event;
pub mod packet;
#[cfg(test)]
mod test_support;
pub mod traits;
pub mod transport;
pub mod types;
pub mod utils;
