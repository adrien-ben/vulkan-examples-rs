#version 450

layout(location = 0) in vec4 oColor;

layout(location = 0) out vec4 finalColor;

void main() {
    finalColor = oColor;
}
