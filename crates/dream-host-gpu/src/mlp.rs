//! MLP layers for `GpuMlpWeights.apply`: one storage buffer per layer, in the matrix layout the
//! device's shader code multiplies with.

use crate::backend::Backend;
use crate::error::{HostError, HostResult, check, feature};
use crate::ffi;
use crate::resources;
use crate::state::{self, DeviceCore};
use std::borrow::Cow;

/// How a layer's weight matrix is stored. It must agree with `DREAM_MLP_LAYOUT`, which the
/// shader generator fixes per target in the generated Slang.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MlpLayout {
    /// Rows of `inputs` values, tightly packed.
    RowMajor,
    /// The driver's own layout for cooperative-vector math, sized by the driver.
    InferencingOptimal,
}

/// SPIR-V and DXIL code multiplies in the inferencing-optimal layout, which only a device with
/// cooperative vectors can produce, and only such a device can run that code. PTX is compiled
/// without OptiX, so Slang emulates cooperative vectors there and reads row-major matrices.
/// Metal and WebGPU code has no cooperative vectors at all.
pub fn layout_for(backend: Backend, cooperative_vectors: bool) -> MlpLayout {
    match backend {
        Backend::Vulkan | Backend::Dx12 if cooperative_vectors => MlpLayout::InferencingOptimal,
        _ => MlpLayout::RowMajor,
    }
}

/// The biases come first and the matrix starts at the next multiple of 64 bytes, the matrix
/// alignment cooperative vectors need. Both offsets are then known when the shader is built,
/// whatever size the device's matrix layout takes.
pub fn matrix_offset(outputs: u32, element: u32) -> u64 {
    (u64::from(outputs) * u64::from(element)).next_multiple_of(64)
}

/// The layer buffer: `biases` at 0 and `matrix` at `offset`.
pub fn pack(biases: &[u8], matrix: &[u8], offset: u64) -> Vec<u8> {
    let offset = offset as usize;
    let mut out = vec![0; offset + matrix.len()];
    out[..biases.len()].copy_from_slice(biases);
    out[offset..].copy_from_slice(matrix);
    out
}

/// One layer as the guest sends it: `layout` is [inputs, outputs, element_bytes] and `data`
/// holds the row-major `outputs x inputs` weights followed by `outputs` biases.
#[derive(Debug, PartialEq, Eq)]
pub struct Layer<'a> {
    pub inputs: u32,
    pub outputs: u32,
    pub element: u32,
    pub weights: &'a [u8],
    pub biases: &'a [u8],
}

impl<'a> Layer<'a> {
    pub fn parse(data: &'a [u8], layout: &[i32]) -> HostResult<Self> {
        let &[inputs, outputs, element] = layout else {
            return Err(HostError::invalid("an MLP layout has 3 values"));
        };
        if inputs <= 0 || outputs <= 0 || !(element == 2 || element == 4) {
            return Err(HostError::invalid("malformed MLP layer layout"));
        }
        let (inputs, outputs, element) = (inputs as u64, outputs as u64, element as u64);
        let matrix = inputs * outputs * element;
        if data.len() as u64 != matrix + outputs * element {
            return Err(HostError::invalid(
                "MLP data must hold inputs * outputs weights and one bias per output",
            ));
        }
        let (weights, biases) = data.split_at(matrix as usize);
        Ok(Self {
            inputs: inputs as u32,
            outputs: outputs as u32,
            element: element as u32,
            weights,
            biases,
        })
    }

    /// slang-rhi's CooperativeVectorComponentType.
    fn component(&self) -> i32 {
        if self.element == 2 { 0 } else { 1 }
    }
}

fn convert_optimal(core: &DeviceCore, layer: &Layer) -> HostResult<Vec<u8>> {
    let mut size = 0;
    check(unsafe {
        ffi::dgpu_mlp_optimal_size(
            core.raw(),
            layer.outputs,
            layer.inputs,
            layer.component(),
            &mut size,
        )
    })?;
    let mut out = vec![0u8; size as usize];
    check(unsafe {
        ffi::dgpu_mlp_convert_optimal(
            core.raw(),
            layer.weights.as_ptr().cast(),
            layer.outputs,
            layer.inputs,
            layer.component(),
            out.as_mut_ptr().cast(),
            size,
        )
    })?;
    Ok(out)
}

pub fn upload(device: i32, data: &[u8], layout: &[i32]) -> HostResult<i32> {
    let layer = Layer::parse(data, layout)?;
    let matrix = {
        let state = state::lock();
        let core = state.device(device)?;
        match layout_for(core.backend, core.has_feature(feature::COOPERATIVE_VECTORS)) {
            MlpLayout::RowMajor => Cow::Borrowed(layer.weights),
            MlpLayout::InferencingOptimal => Cow::Owned(convert_optimal(&core, &layer)?),
        }
    };
    let bytes = pack(
        layer.biases,
        &matrix,
        matrix_offset(layer.outputs, layer.element),
    );
    let size = i32::try_from(bytes.len())
        .map_err(|_| HostError::invalid("the MLP layer does not fit in one buffer"))?;
    let handle = resources::buffer_create(device, size, 1)?;
    if let Err(e) = resources::buffer_write(handle, 0, &bytes) {
        state::lock().release(handle);
        return Err(e);
    }
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_vulkan_and_d3d12_with_cooperative_vectors_repack() {
        for backend in Backend::ALL {
            assert_eq!(
                layout_for(backend, false),
                MlpLayout::RowMajor,
                "{backend:?}"
            );
            let expected = match backend {
                Backend::Vulkan | Backend::Dx12 => MlpLayout::InferencingOptimal,
                _ => MlpLayout::RowMajor,
            };
            assert_eq!(layout_for(backend, true), expected, "{backend:?}");
        }
    }

    #[test]
    fn the_matrix_starts_at_the_next_64_byte_boundary_after_the_biases() {
        assert_eq!(matrix_offset(4, 2), 64);
        assert_eq!(matrix_offset(32, 2), 64);
        assert_eq!(matrix_offset(33, 2), 128);
        assert_eq!(matrix_offset(33, 4), 192);
    }

    #[test]
    fn packing_puts_biases_first_and_pads_up_to_the_matrix() {
        let out = pack(&[1, 2, 3, 4], &[9, 8], 64);
        assert_eq!(out.len(), 66);
        assert_eq!(&out[..4], &[1, 2, 3, 4]);
        assert!(out[4..64].iter().all(|b| *b == 0));
        assert_eq!(&out[64..], &[9, 8]);
    }

    #[test]
    fn layers_split_into_weights_and_biases() {
        let data: Vec<u8> = (0..(3 * 2 + 2) * 2).collect();
        let layer = Layer::parse(&data, &[3, 2, 2]).unwrap();
        assert_eq!((layer.inputs, layer.outputs, layer.element), (3, 2, 2));
        assert_eq!(layer.weights.len(), 12);
        assert_eq!(layer.biases, &[12, 13, 14, 15]);
        assert_eq!(layer.component(), 0);
    }

    #[test]
    fn malformed_layers_are_rejected() {
        let data = [0u8; 16];
        assert!(Layer::parse(&data, &[3, 2]).is_err());
        assert!(Layer::parse(&data, &[0, 2, 2]).is_err());
        assert!(Layer::parse(&data, &[3, 2, 8]).is_err());
        assert!(Layer::parse(&data, &[3, 2, 4]).is_err());
    }
}
