#version 450

layout(location = 0) in vec3 vPosition;

layout(binding = 0, set = 0) uniform Ubo {
  mat4 projectionViewMatrix;
} ubo;

layout(location = 0) out vec3 oPosition;

void main() {
    oPosition = vPosition;

    gl_Position = ubo.projectionViewMatrix * vec4(vPosition, 1.0);
}
