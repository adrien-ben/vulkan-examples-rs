# HDR Skybox

Displays an HDR Skybox.

IN PROGRESS: I update this example as I learn more about HDR displays and how to target then with Vulkan.

## Controls

- Right Click + move mouse to rotate the camera
- Use the UI controls to toggle HDR, load another HDR image, change tone mapping or switch to calibration mode

## Rendering

When started, the sample targets an SDR swapchain. If HDR support is detected (physical device reports the expected surface format)
the "Enable HDR" checkbox can be toggled.

### Swapchain format

In HDR mode the swapchain use a R16G16B16A16_SFLOAT format with an EXTENDED_SRGB_LINEAR_EXT color space.
This is what is recommanded in [1].

### Skybox 

The skybox pass is simple and just renders a 3D skybox from an equirectangular HDR image to a RGBA16_SFLOAT framebuffer.

### Tonemapping

This pass takes the skybox framebuffer as input and applies a user selected tone mapping filter to it.

> For now you can apply either no mapping or a simple ACES filter from [2] (or [3] in SDR). The goal is to add more at some point.
At least one taking advantage of the calibration values.

### Calibration

When in calibration mode the screen is split vertically. The right part displays a reference value that
the user must match to find its display's minimum and maximum brightness.

[1] explains how to map rgb white values to brightness values in nits.

> Calibration mode is only available when HDR is active.

## Tests 

The example has been tested on:

- a Windows 11 machine running a RTX 4080 and plugged to an Oled HDR display.
- a Steam Deck Oled (Game Mode only, in Desktop Mode the driver does not report the expected surface format).


## References

- [windows' high-dynamic-range][1]
- [hdr-display-first-steps][2]
- [aces-filmic-tone-mapping-curve][3]

[1]: https://learn.microsoft.com/en-us/windows/win32/direct3darticles/high-dynamic-range
[2]: https://knarkowicz.wordpress.com/2016/08/31/hdr-display-first-steps/
[3]: https://knarkowicz.wordpress.com/2016/01/06/aces-filmic-tone-mapping-curve/
