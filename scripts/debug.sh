export VK_LAYER_PATH=$VULKAN_SDK/Bin
export VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation
export RUST_LOG=DEBUG
export RUST_BACKTRACE=1

cargo run --bin $1  -p $1

export VK_LAYER_PATH=
export VK_INSTANCE_LAYERS=
export RUST_LOG=
export RUST_BACKTRACE=
