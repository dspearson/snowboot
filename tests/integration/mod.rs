// Integration test module

mod config_test;
mod validation_test;
mod connection_test;

// Helper function to find an available port for tests
pub fn find_available_port() -> u16 {
    use std::net::TcpListener;

    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to a port")
        .local_addr()
        .expect("Failed to get local address")
        .port()
}
