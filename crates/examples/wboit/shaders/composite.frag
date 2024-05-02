#version 450

layout(location = 0) in vec2 oUV;

layout(binding = 0, set = 0) uniform sampler2D weightedColors;
layout(binding = 1, set = 0) uniform sampler2D reveal;

layout(location = 0) out vec4 finalColor;

// epsilon number
const float EPSILON = 0.00001f;

// calculate floating point numbers equality accurately
bool isApproximatelyEqual(float a, float b) {
    return abs(a - b) <= (abs(a) < abs(b) ? abs(b) : abs(a)) * EPSILON;
}

// get the max value between three values
float max3(vec3 v) {
    return max(max(v.x, v.y), v.z);
}

void main() {
    float reveal = texture(reveal, oUV).r;

    // save the blending and color texture fetch cost if there is not a transparent fragment
    if (isApproximatelyEqual(reveal, 1.0f))
        discard;

    vec4 weightedColor = texture(weightedColors, oUV);

    // suppress overflow
    if (isinf(max3(abs(weightedColor.rgb))))
        weightedColor.rgb = vec3(weightedColor.a);

    // prevent floating point precision bug
    vec3 averageColor = weightedColor.rgb / max(weightedColor.a, EPSILON);

    // blend pixels
    finalColor = vec4(averageColor, 1.0 - reveal);
}
