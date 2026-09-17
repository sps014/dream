//! WGSL definitions for operations that have no WGSL builtin.
//!
//! These are emitted as functions rather than inlined at the call site: the bodies repeat their
//! argument dozens of times, so inlining would re-evaluate the caller's expression once per
//! mention. The bodies deliberately mirror `GpuMath` in `gpu_math.dream` line for line — same
//! `r<row><col>` names over the same column-major storage — so the CPU and shader answers can be
//! compared by reading them side by side.

/// Prefixes `wgsl` with the definitions it calls.
///
/// Driven by the emitted text rather than a scan of the AST, so a call site cannot be missed: if
/// the name appears, the definition goes in. WGSL needs a function declared before it is used, and
/// these depend only on builtins and each other, so the top of the module is always a valid spot.
pub(super) fn prepend_intrinsics(wgsl: String) -> String {
    if !wgsl.contains("dream_inverse_mat") {
        return wgsl;
    }
    format!("{INVERSE_WGSL}\n{wgsl}")
}

/// `inverse` for each matrix size, keyed off the one name the Dream side exposes.
///
/// All three are emitted whenever `inverse` is called, because picking just the needed one would
/// mean inferring the argument type during the scan pass. Unused WGSL functions are legal and the
/// shader compiler drops them.
const INVERSE_WGSL: &str = r#"
fn dream_inverse_mat2(m: mat2x2<f32>) -> mat2x2<f32> {
  let det = determinant(m);
  if (det == 0.0) { return mat2x2<f32>(vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0)); }
  let d = 1.0 / det;
  return mat2x2<f32>(
    vec2<f32>(m[1][1] * d, -m[0][1] * d),
    vec2<f32>(-m[1][0] * d, m[0][0] * d));
}

fn dream_inverse_mat3(m: mat3x3<f32>) -> mat3x3<f32> {
  let det = determinant(m);
  if (det == 0.0) {
    return mat3x3<f32>(
      vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0));
  }
  let d = 1.0 / det;
  return mat3x3<f32>(
    vec3<f32>(
      (m[1][1] * m[2][2] - m[2][1] * m[1][2]) * d,
      (m[2][1] * m[0][2] - m[0][1] * m[2][2]) * d,
      (m[0][1] * m[1][2] - m[1][1] * m[0][2]) * d),
    vec3<f32>(
      (m[2][0] * m[1][2] - m[1][0] * m[2][2]) * d,
      (m[0][0] * m[2][2] - m[2][0] * m[0][2]) * d,
      (m[1][0] * m[0][2] - m[0][0] * m[1][2]) * d),
    vec3<f32>(
      (m[1][0] * m[2][1] - m[2][0] * m[1][1]) * d,
      (m[2][0] * m[0][1] - m[0][0] * m[2][1]) * d,
      (m[0][0] * m[1][1] - m[1][0] * m[0][1]) * d));
}

fn dream_cofactors_mat4(m: mat4x4<f32>) -> mat4x4<f32> {
  let r00 = m[0][0]; let r01 = m[1][0]; let r02 = m[2][0]; let r03 = m[3][0];
  let r10 = m[0][1]; let r11 = m[1][1]; let r12 = m[2][1]; let r13 = m[3][1];
  let r20 = m[0][2]; let r21 = m[1][2]; let r22 = m[2][2]; let r23 = m[3][2];
  let r30 = m[0][3]; let r31 = m[1][3]; let r32 = m[2][3]; let r33 = m[3][3];

  let s0 = r20 * r31 - r30 * r21;
  let s1 = r20 * r32 - r30 * r22;
  let s2 = r20 * r33 - r30 * r23;
  let s3 = r21 * r32 - r31 * r22;
  let s4 = r21 * r33 - r31 * r23;
  let s5 = r22 * r33 - r32 * r23;
  let t0 = r00 * r11 - r10 * r01;
  let t1 = r00 * r12 - r10 * r02;
  let t2 = r00 * r13 - r10 * r03;
  let t3 = r01 * r12 - r11 * r02;
  let t4 = r01 * r13 - r11 * r03;
  let t5 = r02 * r13 - r12 * r03;

  return mat4x4<f32>(
    vec4<f32>(
      r11 * s5 - r12 * s4 + r13 * s3,
      -(r10 * s5) + r12 * s2 - r13 * s1,
      r10 * s4 - r11 * s2 + r13 * s0,
      -(r10 * s3) + r11 * s1 - r12 * s0),
    vec4<f32>(
      -(r01 * s5) + r02 * s4 - r03 * s3,
      r00 * s5 - r02 * s2 + r03 * s1,
      -(r00 * s4) + r01 * s2 - r03 * s0,
      r00 * s3 - r01 * s1 + r02 * s0),
    vec4<f32>(
      r31 * t5 - r32 * t4 + r33 * t3,
      -(r30 * t5) + r32 * t2 - r33 * t1,
      r30 * t4 - r31 * t2 + r33 * t0,
      -(r30 * t3) + r31 * t1 - r32 * t0),
    vec4<f32>(
      -(r21 * t5) + r22 * t4 - r23 * t3,
      r20 * t5 - r22 * t2 + r23 * t1,
      -(r20 * t4) + r21 * t2 - r23 * t0,
      r20 * t3 - r21 * t1 + r22 * t0));
}

fn dream_inverse_mat4(m: mat4x4<f32>) -> mat4x4<f32> {
  let c = dream_cofactors_mat4(m);
  let det = m[0][0] * c[0][0] + m[0][1] * c[1][0] + m[0][2] * c[2][0] + m[0][3] * c[3][0];
  if (det == 0.0) {
    return mat4x4<f32>(
      vec4<f32>(1.0, 0.0, 0.0, 0.0), vec4<f32>(0.0, 1.0, 0.0, 0.0),
      vec4<f32>(0.0, 0.0, 1.0, 0.0), vec4<f32>(0.0, 0.0, 0.0, 1.0));
  }
  let d = 1.0 / det;
  return mat4x4<f32>(c[0] * d, c[1] * d, c[2] * d, c[3] * d);
}
"#;
