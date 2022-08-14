export RUST_LOG=INFO

cargo run --bin $1 -p $1 --release

export RUST_LOG=