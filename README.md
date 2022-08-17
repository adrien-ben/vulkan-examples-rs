# vulkan-examples-rs

Collection of Vulkan examples that I make to learn stuff in Rust using [ash][ash].

![screenshot](media/screenshot.png "Screenshot")

## Examples

You can run one of the following example.

- rt_triangle: Ray-traced triangle.
- rt_shadows: Ray-traced gltf model with simulated sunlight shadow. It has one BLAS with multiple geometries. Light and camera controls with imgui.
- rt_reflections: Ray-traced iterative (not recursive) reflections.
- triangle: Rasterized triangle.
- gpu_particles: Particles simulated on the gpu using a compute shader.

```ps1
# Powershell example (all scripts have a .sh version)

# Compile all glsl shaders to spir-v 1.4
.\scripts\compile_shaders.ps1

# Enable validation layers and set log level to debug
.\scripts\debug.ps1 <example>

# Compiles with --release and set log level to info
.\scripts\run.ps1 <example>
```

## Controls

For examples with interactive camera you can move the camera with 
- WASD to move
- Ctrl and space to go up or down
- Right-click and move the mouse around to look

Also you can toggle the stats window by pressing R.

## Requirements

All examples use Vulkan 1.3 and the following features:

- dynamic_rendering
- synchronization2

Ray tracing examples use the following extensions and features:

- VK_KHR_ray_tracing_pipeline
    - ray_tracing_pipeline
- VK_KHR_acceleration_structure
    - acceleration_structure
- VK_KHR_deferred_host_operations
- Vulkan 1.2's features
    - runtime_descriptor_array
    - buffer_device_address

> RT is only enabled on examples using it, so other examples can run on hardware that does not support it.

## Useful links

- [NVidia tutorial](https://nvpro-samples.github.io/vk_raytracing_tutorial_KHR/)
- [SaschaWillems' Vulkan](https://github.com/SaschaWillems/Vulkan)

[ash]: https://github.com/MaikKlein/ash
