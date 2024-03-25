#version 450

layout(location = 0) in vec2 oUV;

layout(binding = 0, set = 0) uniform Ubo {
    float userNits;
    float referenceNits;
} ubo;

layout(location = 0) out vec4 finalColor;

// https://learn.microsoft.com/en-us/windows/win32/direct3darticles/high-dynamic-range#step-3-perform-the-hdr-tonemapping-operation
const float NITS_TO_WHITE = 12.5 / 1000.0;

void main() {
    if (oUV.x < 0.5) {
        finalColor = vec4(vec3(ubo.userNits * NITS_TO_WHITE), 1.0);
    } else {
        finalColor = vec4(vec3 (ubo.referenceNits * NITS_TO_WHITE), 1.0);
    }
}
