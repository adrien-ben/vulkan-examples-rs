#version 450

layout(location = 0) in vec2 oUV;

layout(binding = 0, set = 0) uniform sampler2D hdrFramebuffer;
layout(binding = 1, set = 0) uniform sampler2D uiSampler;

layout(location = 0) out vec4 finalColor;

void main() {
    vec4 texColor = texture(hdrFramebuffer, oUV);
    vec4 uiColor = texture(uiSampler, oUV);
    
    finalColor = vec4(
        uiColor.r + (1.0 - uiColor.a) * texColor.r,
        uiColor.g + (1.0 - uiColor.a) * texColor.g,
        uiColor.b + (1.0 - uiColor.a) * texColor.b,
        1.0
    );
}
