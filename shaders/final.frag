#version 450

// Genius Level Upgrade: Re-interpreting inputs as a Physically Based Rendering (PBR) G-Buffer
// This is the standard for modern, high-fidelity deferred rendering pipelines.
layout(input_attachment_index = 0, set = 0, binding = 0) uniform subpassInput gBufferAlbedo;      // Albedo (RGB) + Opacity (A)
layout(input_attachment_index = 1, set = 0, binding = 1) uniform subpassInput gBufferNormal;      // World-space Normal (XYZ)
layout(input_attachment_index = 2, set = 0, binding = 2) uniform subpassInput gBufferMaterial;    // Metallic(R), Roughness(G), Ambient Occlusion(B)
layout(input_attachment_index = 3, set = 0, binding = 3) uniform subpassInput lightingResult;     // Final calculated light color (HDR)

layout(location = 0) out vec4 f_color;

// --- CONSTANTS ---
const float PI = 3.14159265359;

// --- PBR HELPER FUNCTIONS (Cook-Torrance BRDF) ---
// These functions mathematically model how light interacts with a surface.

// Trowbridge-Reitz GGX Normal Distribution Function (NDF)
// Models the alignment of microscopic surface details ("microfacets").
float D_GGX(float NdotH, float roughness) {
    float a = roughness * roughness;
    float a2 = a * a;
    float d = (NdotH * NdotH * (a2 - 1.0) + 1.0);
    return a2 / (PI * d * d);
}

// Smith's method with Schlick-GGX for Geometry Function (G)
// Models the self-shadowing and masking of the microfacets.
float G_SchlickGGX(float NdotV, float k) {
    return NdotV / (NdotV * (1.0 - k) + k);
}

float G_Smith(float NdotV, float NdotL, float roughness) {
    float r = roughness + 1.0;
    float k = (r * r) / 8.0; // k for direct lighting
    return G_SchlickGGX(NdotV, k) * G_SchlickGGX(NdotL, k);
}

// Fresnel Equation (Schlick's Approximation)
// Describes how light reflects at different viewing angles. Metals are highly reflective,
// while non-metals like plastic reflect more at grazing angles.
vec3 F_Schlick(float cosTheta, vec3 F0) {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}


// --- TONE MAPPING & COLOR GRADING ---
// ACES Filmic Tone Mapping: The industry standard for mapping HDR colors to
// the screen, creating a cinematic look with rich contrast.
vec3 toneMapACES(vec3 color) {
    // Optimized ACES approximation
    const float A = 2.51;
    const float B = 0.03;
    const float C = 2.43;
    const float D = 0.59;
    const float E = 0.14;
    color = (color * (A * color + B)) / (color * (C * color + D) + E);
    // Gamma correction
    return pow(color, vec3(1.0 / 2.2));
}

// --- MAIN SHADER LOGIC ---
void main()
{
    // 1. Deconstruct G-Buffer Inputs
    // Load all material properties from the previous render subpasses.
    vec3 albedo     = subpassLoad(gBufferAlbedo).rgb;
    vec3 normal     = normalize(subpassLoad(gBufferNormal).xyz);
    float metallic  = subpassLoad(gBufferMaterial).r;
    float roughness = subpassLoad(gBufferMaterial).g;
    float ao        = subpassLoad(gBufferMaterial).b; // Ambient Occlusion

    // In a real engine, view direction is derived from a camera uniform buffer and
    // the fragment's world position (reconstructed from depth).
    // For this example, we assume a view from directly in front.
    vec3 V = normalize(vec3(0.0, 0.0, 1.0));

    // Assume lighting pass provides a single dominant light source for simplicity.
    // In a full engine, this would be the sum of all lights.
    vec3 L          = normalize(vec3(0.8, 0.9, 0.6)); // Example static light direction
    vec3 lightColor = subpassLoad(lightingResult).rgb; // Radiance (HDR light value)

    // Calculate essential vectors
    vec3 H      = normalize(V + L); // Halfway vector
    float NdotL = max(dot(normal, L), 0.0);
    float NdotV = max(dot(normal, V), 0.0);
    float NdotH = max(dot(normal, H), 0.0);
    float HdotV = max(dot(H, V), 0.0);

    // 2. Calculate Surface Reflectivity (F0)
    // For non-metals (dielectrics), F0 is a constant low value. For metals, F0 is their albedo color.
    vec3 F0 = mix(vec3(0.04), albedo, metallic);

    // 3. Calculate Cook-Torrance BRDF terms
    float D   = D_GGX(NdotH, roughness);
    float G   = G_Smith(NdotV, NdotL, roughness);
    vec3  F   = F_Schlick(HdotV, F0);
    vec3  specularNumerator = D * G * F;
    float specularDenominator = 4.0 * NdotV * NdotL + 0.001; // Epsilon prevents division by zero
    vec3 specular = specularNumerator / specularDenominator;

    // 4. Calculate Diffuse and Specular contributions (with Energy Conservation)
    vec3 kS = F; // Specular ratio is determined by the Fresnel term
    vec3 kD = (vec3(1.0) - kS) * (1.0 - metallic); // Diffuse ratio conserves energy and is 0 for pure metals
    vec3 diffuse = kD * albedo / PI;

    // 5. Combine Lighting
    // The final lit color is the sum of diffuse and specular reflections, scaled by the incoming light.
    vec3 outgoingLight = (diffuse + specular) * lightColor * NdotL;

    // 6. Apply Ambient Light
    // A sophisticated engine would use Image-Based Lighting (IBL) here.
    // We use a simple ambient term modulated by the AO map.
    vec3 ambient = vec3(0.05) * albedo * ao;
    vec3 color = ambient + outgoingLight;

    // 7. Final Output Stage: HDR Tone Mapping
    // This crucial step transforms the calculated HDR color into a visually pleasing
    // LDR color suitable for display, preventing washed-out highlights.
    f_color = vec4(toneMapACES(color), 1.0);
}
