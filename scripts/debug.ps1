$env:VK_LAYER_PATH = "$env:VULKAN_SDK\Bin"
$env:VK_INSTANCE_LAYERS = "VK_LAYER_KHRONOS_validation"
$env:RUST_LOG="DEBUG"

cargo run --bin $args[0]

$env:VK_LAYER_PATH = ""
$env:VK_INSTANCE_LAYERS = ""
$env:RUST_LOG=""
