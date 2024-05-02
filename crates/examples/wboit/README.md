# Weighted, Blended Order-Independent Transparency.

Implementation of one of many order independent transparency algorithm.
I mostly followed and adapted what is described in [1].

## Controls

- Right Click + move mouse to rotate the camera
- WASD to move
- Use the UI controls to add squares and change their position and color

## Requirements

In addition of the [common requirements](../../../README.md#requirements) the device needs to support
the `independentBlend` feature.

## What it does

Renders a few squares which position and color can be edited.

First it render opaque squares (alpha = 1.0) to the swapchain.

Then transparent squares are rendered. This pass used two framebuffers with different 
blending configurations. The first is RGBA16_SFLOAT, contains the weighed color accumulation 
and uses a ADD ONE/ONE blend function. 
The second one (R8_UNORM), contains the "revealage" for each fragment and uses a 
ADD ZERO/ONE_MINUS_SRC_COLOR blend function.

Next these framebuffer are used as input of a composition pass that computes the final fragment
color for the transparent pass. This targets the swapchain using ADD SRC_ALPHA/ONE_MINUS_SRC_ALPHA
as blend function.

## References

[learnopengl][1]
[nvpro-samples][2]
[paper][3]

[1]: https://learnopengl.com/Guest-Articles/2020/OIT/Weighted-Blended
[2]: https://github.com/nvpro-samples/vk_order_independent_transparency
[3]: https://jcgt.org/published/0002/02/09/
