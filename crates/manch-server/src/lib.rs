//! Manch ConnectRPC server. Embeds the Manch core (a stub for now) and exposes
//! it over the Connect / gRPC-web protocol.

pub mod proto {
    connectrpc::include_generated!();
}

pub mod service;

pub use service::ManchServiceImpl;
