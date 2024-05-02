#version 450

layout(location = 0) in vec3 vPosition;

layout(binding = 0, set = 0) uniform FrameUbo {
    mat4 projectionViewMatrix;
} frameUbo;

layout(binding = 1, set = 0) uniform InstanceUbo {
    vec4 color;
    vec3 position;
} instanceUbo;

layout(location = 0) out vec4 oColor;

void main() {
    oColor = instanceUbo.color;

    gl_Position = frameUbo.projectionViewMatrix * vec4(vPosition + instanceUbo.position, 1.0);
}
