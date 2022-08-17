#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec3 vPosition;
layout(location = 1) in vec4 vColor;

layout(binding = 0) uniform Ubo {
  mat4 projectionViewMatrix;
  float particleSize;
} ubo;

layout(location = 0) out vec4 oColor;

void main() {
    oColor = vColor;

    gl_PointSize = ubo.particleSize;
    gl_Position = ubo.projectionViewMatrix * vec4(vPosition.x, vPosition.y, vPosition.z, 1.0);
}
