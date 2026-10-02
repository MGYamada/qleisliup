// Both executables share the private implementation, without a public library.
// The Cargo binary name selects bootstrap behavior, never a runtime override.
include!("main.rs");
