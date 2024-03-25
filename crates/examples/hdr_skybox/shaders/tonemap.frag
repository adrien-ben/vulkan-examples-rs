#version 450

layout(location = 0) in vec2 oUV;

layout(binding = 0, set = 0) uniform sampler2D skyboxSampler;

layout(binding = 1, set = 0) uniform Ubo {
  int toneMapMode;
} ubo;

layout(location = 0) out vec4 finalColor;

const int TONEMAP_MODE_ACESFILMREC2020 = 1;
const int TONEMAP_MODE_ACESFILM = 2;

// https://knarkowicz.wordpress.com/2016/08/31/hdr-display-first-steps/
vec3 ACESFilmRec2020(vec3 x) {
    float a = 15.8f;
    float b = 2.12f;
    float c = 1.2f;
    float d = 5.92f;
    float e = 1.9f;
    return ( x * ( a * x + b ) ) / ( x * ( c * x + d ) + e );
}

// https://knarkowicz.wordpress.com/2016/01/06/aces-filmic-tone-mapping-curve/
vec3 ACESFilm(vec3 x) {
    float a = 2.51f;
    float b = 0.03f;
    float c = 2.43f;
    float d = 0.59f;
    float e = 0.14f;
    return clamp((x*(a*x+b))/(x*(c*x+d)+e), 0.0, 1.0);
}

void main() {
    vec4 texColor = texture(skyboxSampler, oUV);

    vec3 tonemapped = texColor.rgb;
    if (ubo.toneMapMode == TONEMAP_MODE_ACESFILMREC2020) {
        tonemapped = ACESFilmRec2020(texColor.rgb);
    } else if (ubo.toneMapMode == TONEMAP_MODE_ACESFILM) {
        tonemapped = ACESFilm(texColor.rgb);
    }

    finalColor = vec4(tonemapped, 1.0);
}
