#version 450

layout(location = 0) in vec3 oPosition;

layout(binding = 1, set = 0) uniform sampler2D textureSampler;

layout(location = 0) out vec4 finalColor;

const vec2 INV_ATAN = vec2(0.1591, 0.3183);
vec2 sampleShericalMap(vec3 position) {
    return 0.5 + (vec2(atan(position.z, position.x), asin(-position.y)) * INV_ATAN);
}

void main() {
    vec2 uv = sampleShericalMap(normalize(oPosition));
    finalColor = texture(textureSampler, uv);
}
