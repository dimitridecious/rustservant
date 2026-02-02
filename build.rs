fn main() {
    // This runs at compile time and generates Rust code from rustplus.proto
    prost_build::compile_protos(&["rustplus.proto"], &["."])
        .expect("Failed to compile protobuf files");
}
