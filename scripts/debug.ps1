$env:VK_LAYER_PATH = "$env:VULKAN_SDK\Bin"
$env:VK_INSTANCE_LAYERS = "VK_LAYER_KHRONOS_validation"
$env:RUST_LOG="DEBUG"
$env:RUST_BACKTRACE="1"

cargo run --bin $args[0] -p $args[0]

$env:VK_LAYER_PATH = ""
$env:VK_INSTANCE_LAYERS = ""
$env:RUST_LOG=""
$env:RUST_BACKTRACE=""
