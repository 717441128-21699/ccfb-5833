// WGSL 着色器：主渲染（PBR + 阴影 + SSAO 调制 + 爆炸/拆装位移）、
// 深度预渲染（阴影与 SSAO 共用几何）、SSAO 计算与模糊。

// ---------- 公共 ----------
struct Camera {
  viewProj: mat4x4<f32>,
  lightViewProj: mat4x4<f32>,
  camPos: vec3<f32>,
  _pad: f32,
  lightDir: vec3<f32>,
  _pad2: f32,
};
struct Dynamic {
  explodeScale: f32,   // 爆炸视图整体位移距离（米）
  disassemble: f32,    // 拆装进度 0–1
  maxLayer: f32,
  time: f32,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> dyn: Dynamic;

struct VsIn {
  @location(0) position: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) matId: u32,
  @location(3) compId: u32,
  @location(4) layer: u32,
  @location(5) explodeDir: vec3<f32>,
};
struct VsOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) world: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) uv: vec2<f32>,
  @location(3) @interpolate(flat) matId: u32,
  @location(4) shadow: vec4<f32>,
};

fn layerFactor(layer: f32, progress: f32, maxLayer: f32) -> f32 {
  let window = 1.0 / (maxLayer + 1.0);
  let start = layer * window * 0.85;
  let local = clamp((progress - start) / (window * 1.6), 0.0, 1.0);
  return local * local * (3.0 - 2.0 * local);
}

fn displace(in: VsIn) -> vec3<f32> {
  let lf = layerFactor(f32(in.layer), dyn.disassemble, max(dyn.maxLayer, 1.0));
  // 拆装：沿构件自身拆解方向，按层号拉开（层越高走得越远）。
  let disDist = lf * (0.18 + f32(in.layer) * 0.16);
  // 爆炸：整体沿 explodeDir 外推。
  return in.position + in.explodeDir * (disDist + dyn.explodeScale);
}

// ---------- 主顶点着色器 ----------
@vertex
fn vs_main(in: VsIn) -> VsOut {
  let world = displace(in);
  var out: VsOut;
  out.clip = camera.viewProj * vec4<f32>(world, 1.0);
  out.world = world;
  out.normal = in.normal;
  // 稳定的虚拟 UV：按世界坐标平铺，每个构件表面纹理连续。
  out.uv = vec2<f32>(world.x * 5.0 + world.y * 5.0, world.z * 5.0);
  out.matId = in.matId;
  out.shadow = camera.lightViewProj * vec4<f32>(world, 1.0);
  return out;
}

// ---------- 阴影 / 深度顶点着色器 ----------
@vertex
fn vs_shadow(in: VsIn) -> @builtin(position) vec4<f32> {
  let world = displace(in);
  return camera.lightViewProj * vec4<f32>(world, 1.0);
}

@vertex
fn vs_depth(in: VsIn) -> @builtin(position) vec4<f32> {
  let world = displace(in);
  return camera.viewProj * vec4<f32>(world, 1.0);
}

// ---------- 材质片元着色器 ----------
@group(1) @binding(0) var colorTex: texture_2d<f32>;
@group(1) @binding(1) var colorSampler: sampler;
@group(2) @binding(0) var shadowMap: texture_depth_2d;
@group(2) @binding(1) var aoMap: texture_2d<f32>;
@group(2) @binding(2) var aoSampler: sampler;
@group(2) @binding(3) var<uniform> screenSize: vec2<f32>;
@group(2) @binding(4) var<uniform> _ssPad: vec2<f32>;

// xyz = 基色微调，w = 粗糙度。
@group(3) @binding(0) var<uniform> materials: array<vec4<f32>, 8>;

fn sampleShadow(shadow: vec4<f32>) -> f32 {
  let ndc = shadow.xyz / shadow.w;
  let uv = ndc.xy * 0.5 + 0.5;
  if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || ndc.z > 1.0) {
    return 1.0;
  }
  let texSize = vec2<f32>(textureDimensions(shadowMap));
  let texel = vec2<f32>(1.5) / texSize;
  let refDepth = ndc.z;
  var sum = 0.0;
  for (var y = -1; y <= 1; y = y + 1) {
    for (var x = -1; x <= 1; x = x + 1) {
      let px = vec2<i32>((uv + vec2<f32>(f32(x), f32(y)) * texel) * texSize);
      let d = textureLoad(shadowMap, px, 0);
      // 阴影最暗只降到 0.72，保留环境光下的细节，避免死黑。
      sum = sum + select(0.72, 1.0, refDepth - 0.0025 <= d);
    }
  }
  return sum / 9.0;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
  let m = materials[in.matId];
  let texColor = textureSample(colorTex, colorSampler, in.uv).rgb;
  // 纹理已带完整基色，材质 uniform 只做轻微色调调制（向 m.rgb 靠 35%），
  // 避免纹理色与基色全强度相乘把暗部压成死黑。
  let albedo = texColor * mix(vec3<f32>(1.0), m.rgb, 0.35);
  let roughness = m.a;

  let nRaw = normalize(in.normal);
  let v = normalize(camera.camPos - in.world);
  // 双面光照：若法线背向相机（绕序翻转的面），翻转法线保证正面受光。
  let n = select(-nRaw, nRaw, dot(nRaw, v) >= 0.0);
  // 主光用固定方向；额外加一盏从相机方向来的补光，保证朝相机的面不死黑。
  let l = normalize(-camera.lightDir);
  let fill = normalize(v + vec3<f32>(0.0, 0.0, 0.4));
  let h = normalize(l + v);

  let ndl = max(dot(n, l), 0.0);
  let ndf = max(dot(n, fill), 0.0);
  let ndh = max(dot(n, h), 0.0);
  let ndv = max(dot(n, v), 0.0);

  // 简化 PBR：Lambert 漫反射 + GGX 高光。
  let a2 = roughness * roughness * roughness * roughness;
  let ggx = a2 / (3.14159265 * pow(ndh * ndh * (a2 - 1.0) + 1.0, 2.0));
  let spec = ggx * 0.35 / max(ndv, 0.08);

  let shadow = sampleShadow(in.shadow);
  let aoUv = in.clip.xy / screenSize;
  let ao = textureSample(aoMap, aoSampler, aoUv).r;

  // 半球环境光：法线朝上偏天空暖色，朝下偏地面冷色。
  let hemiMix = n.z * 0.5 + 0.5;
  let sky = vec3<f32>(0.62, 0.65, 0.70);
  let ground = vec3<f32>(0.42, 0.38, 0.33);
  let ambient = mix(ground, sky, hemiMix) * ao;
  let sun = vec3<f32>(1.0, 0.96, 0.88) * (ndl * shadow);
  let fillLight = vec3<f32>(0.85, 0.82, 0.78) * ndf * 0.5;
  // 环境光保底足够强，背光面只变暗不变死黑。
  let color = albedo * (ambient + fillLight + sun + vec3<f32>(0.12))
    + vec3<f32>(spec) * shadow;

  // Reinhard tonemap（线性），再手动线性→sRGB 编码（画布为非 srgb 格式）。
  let mapped = color / (color + vec3<f32>(1.0)) * 1.2;
  let srgb = select(1.055 * pow(mapped, vec3<f32>(1.0 / 2.4)) - 0.055,
                     mapped * 12.92, mapped <= vec3<f32>(0.0031308));
  return vec4<f32>(clamp(srgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

// ---------- SSAO ----------
struct AoParams {
  proj: mat4x4<f32>,
  invProj: mat4x4<f32>,
  screenSize: vec2<f32>,
  radius: f32,
  _pad: f32,
};
@group(0) @binding(2) var<uniform> aoParams: AoParams;
@group(2) @binding(5) var depthTex: texture_depth_2d;

fn reconstructWorld(uv: vec2<f32>, depth: f32) -> vec3<f32> {
  let ndc = vec4<f32>(uv * 2.0 - 1.0, depth, 1.0);
  let p = aoParams.invProj * ndc;
  return p.xyz / p.w;
}

@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
  var p = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
    vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
  );
  return vec4<f32>(p[vi], 0.0, 1.0);
}

@fragment
fn fs_ssao(@builtin(position) sp: vec4<f32>) -> @location(0) vec4<f32> {
  let uv = sp.xy / aoParams.screenSize;
  let depth = textureLoad(depthTex, vec2<i32>(sp.xy), 0);
  let origin = reconstructWorld(uv, depth);
  // 导数必须在任何非一致分支之前计算（uniform control flow 要求）。
  let ddx = dpdx(origin);
  let ddy = dpdy(origin);
  let normal = normalize(cross(ddx, ddy));
  if (depth >= 1.0) {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
  }

  var kernel = array<vec3<f32>, 16>(
    vec3<f32>(0.5381, 0.1882, 0.4289), vec3<f32>(0.1389, 0.2488, 0.8247),
    vec3<f32>(0.3372, 0.5679, 0.1239), vec3<f32>(0.8200, 0.3206, 0.2104),
    vec3<f32>(0.1031, 0.8139, 0.2304), vec3<f32>(-0.3624, 0.7519, 0.3401),
    vec3<f32>(-0.4281, 0.2039, 0.7104), vec3<f32>(-0.1834, 0.1039, 0.8401),
    vec3<f32>(0.4201, -0.1039, 0.7201), vec3<f32>(0.2031, -0.4281, 0.7104),
    vec3<f32>(-0.1239, -0.2039, 0.8401), vec3<f32>(-0.3206, -0.2200, 0.7201),
    vec3<f32>(0.1039, -0.8139, 0.2304), vec3<f32>(-0.2039, -0.5679, 0.3401),
    vec3<f32>(0.3624, -0.7519, 0.3401), vec3<f32>(-0.5381, -0.1882, 0.4289),
  );

  var occlusion = 0.0;
  let radius = aoParams.radius;
  for (var i = 0; i < 16; i = i + 1) {
    var sampleDir = kernel[i];
    if (dot(sampleDir, normal) < 0.0) { sampleDir = -sampleDir; }
    let samplePos = origin + sampleDir * radius;
    let offset = aoParams.proj * vec4<f32>(samplePos, 1.0);
    let sampleUv = (offset.xy / offset.w) * 0.5 + 0.5;
    if (sampleUv.x < 0.0 || sampleUv.x > 1.0 || sampleUv.y < 0.0 || sampleUv.y > 1.0) {
      continue;
    }
    let sampleDepth = textureLoad(depthTex, vec2<i32>(sampleUv * aoParams.screenSize), 0);
    let occPos = reconstructWorld(sampleUv, sampleDepth);
    let rangeCheck = smoothstep(0.0, 1.0, radius / max(abs(origin.z - occPos.z), 0.0001));
    occlusion = occlusion + select(0.0, 1.0, occPos.z >= samplePos.z + 0.002) * rangeCheck;
  }
  let ao = 1.0 - occlusion / 16.0 * 0.85;
  return vec4<f32>(vec3<f32>(ao), 1.0);
}

// ---------- SSAO 模糊 ----------
// 复用 group(0) 组号：该通道使用独立 pipeline layout，组号可与主通道重叠。
@group(0) @binding(0) var aoIn: texture_2d<f32>;
@group(0) @binding(1) var aoInSampler: sampler;
@group(0) @binding(2) var<uniform> blurScreen: vec2<f32>;

@fragment
fn fs_blur(@builtin(position) sp: vec4<f32>) -> @location(0) vec4<f32> {
  let uv = sp.xy / blurScreen;
  let texel = 1.0 / blurScreen;
  var sum = 0.0;
  for (var y = -2; y <= 2; y = y + 1) {
    for (var x = -2; x <= 2; x = x + 1) {
      sum = sum + textureSample(aoIn, aoInSampler, uv + vec2<f32>(f32(x), f32(y)) * texel).r;
    }
  }
  return vec4<f32>(vec3<f32>(sum / 25.0), 1.0);
}
