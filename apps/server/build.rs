fn main() {
    connectrpc_build::Config::new()
        .files(&["../../proto/manch/v1/manch.proto"])
        .includes(&["../../proto"])
        .include_file("_connectrpc.rs")
        .compile()
        .expect("failed to compile proto");
}
