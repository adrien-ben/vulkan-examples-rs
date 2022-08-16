#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec3 vPosition;
layout(location = 1) in vec3 vColor;

layout(binding = 0) uniform Camera {
  mat4 projectionViewMatrix;
} camera;

layout(location = 0) out vec3 oColor;

void main() {
    oColor = vColor;

    gl_PointSize = 2.0;
    gl_Position = camera.projectionViewMatrix * vec4(vPosition.x, vPosition.y, vPosition.z, 1.0);
}
