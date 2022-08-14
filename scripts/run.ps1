$env:RUST_LOG="INFO"

cargo run --bin $args[0] --release

$env:RUST_LOG=""