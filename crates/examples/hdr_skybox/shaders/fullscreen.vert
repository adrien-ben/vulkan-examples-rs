#version 450

layout(location = 0) in vec2 vPosition;
layout(location = 1) in vec2 vUV;

layout(location = 0) out vec2 oUV;

void main() {
    oUV = vUV;

    gl_Position = vec4(vPosition, 1.0, 1.0);
}
