//! Decoding of `gpuSubmit` command streams (crates/dream-stdlib/src/system/gpu/gpu_op.dream).
//! Only the shape is checked here; handles and pass structure are validated against host state
//! when the stream is recorded.

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    BeginRenderPass {
        color: i32,
        clear: Option<[f32; 4]>,
        depth: i32,
        depth_clear: Option<f32>,
    },
    BeginComputePass,
    EndPass,
    SetPipeline(i32),
    Bind {
        stage: i32,
        index: i32,
        kind: i32,
        a: i32,
        b: i32,
    },
    Dispatch([i32; 3]),
    SetVertexBuffer {
        slot: i32,
        buffer: i32,
        offset: i32,
    },
    SetIndexBuffer {
        buffer: i32,
        format: i32,
        offset: i32,
    },
    SetViewport([f32; 6]),
    SetScissor([i32; 4]),
    Draw([i32; 4]),
    DrawIndexed([i32; 5]),
    CopyBuffer {
        source: i32,
        source_offset: i32,
        destination: i32,
        destination_offset: i32,
        size: i32,
    },
    TraceRays([i32; 3]),
}

fn float(word: i32) -> f32 {
    f32::from_bits(word as u32)
}

/// Operand counts per opcode, indexed by opcode.
const OPERANDS: [usize; 15] = [0, 9, 0, 0, 1, 5, 3, 3, 3, 6, 4, 4, 5, 5, 3];

pub fn decode(words: &[i32]) -> Result<Vec<Op>, String> {
    let mut ops = Vec::new();
    let mut at = 0;
    while at < words.len() {
        let code = words[at];
        let count = usize::try_from(code)
            .ok()
            .filter(|c| (1..OPERANDS.len()).contains(c))
            .map(|c| OPERANDS[c])
            .ok_or_else(|| format!("unknown command {code} at word {at}"))?;
        let Some(w) = words.get(at + 1..at + 1 + count) else {
            return Err(format!("command {code} at word {at} is truncated"));
        };
        ops.push(match code {
            1 => Op::BeginRenderPass {
                color: w[0],
                clear: (w[1] != 0).then(|| [float(w[2]), float(w[3]), float(w[4]), float(w[5])]),
                depth: w[6],
                depth_clear: (w[7] != 0).then(|| float(w[8])),
            },
            2 => Op::BeginComputePass,
            3 => Op::EndPass,
            4 => Op::SetPipeline(w[0]),
            5 => Op::Bind {
                stage: w[0],
                index: w[1],
                kind: w[2],
                a: w[3],
                b: w[4],
            },
            6 => Op::Dispatch([w[0], w[1], w[2]]),
            7 => Op::SetVertexBuffer {
                slot: w[0],
                buffer: w[1],
                offset: w[2],
            },
            8 => Op::SetIndexBuffer {
                buffer: w[0],
                format: w[1],
                offset: w[2],
            },
            9 => Op::SetViewport([
                float(w[0]),
                float(w[1]),
                float(w[2]),
                float(w[3]),
                float(w[4]),
                float(w[5]),
            ]),
            10 => Op::SetScissor([w[0], w[1], w[2], w[3]]),
            11 => Op::Draw([w[0], w[1], w[2], w[3]]),
            12 => Op::DrawIndexed([w[0], w[1], w[2], w[3], w[4]]),
            13 => Op::CopyBuffer {
                source: w[0],
                source_offset: w[1],
                destination: w[2],
                destination_offset: w[3],
                size: w[4],
            },
            _ => Op::TraceRays([w[0], w[1], w[2]]),
        });
        at += 1 + count;
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn bits(value: f32) -> i32 {
        value.to_bits() as i32
    }

    #[test]
    fn decodes_the_frame_stream_the_stdlib_records() {
        let words = [
            1,
            7,
            1,
            bits(0.1),
            bits(0.2),
            bits(0.3),
            bits(1.0),
            0,
            0,
            0,
            4,
            9,
            5,
            1,
            0,
            6,
            0,
            4,
            11,
            3,
            1,
            0,
            0,
            3,
        ];
        let ops = decode(&words).unwrap();
        assert_eq!(
            ops,
            vec![
                Op::BeginRenderPass {
                    color: 7,
                    clear: Some([0.1, 0.2, 0.3, 1.0]),
                    depth: 0,
                    depth_clear: None
                },
                Op::SetPipeline(9),
                Op::Bind {
                    stage: 1,
                    index: 0,
                    kind: 6,
                    a: 0,
                    b: 4
                },
                Op::Draw([3, 1, 0, 0]),
                Op::EndPass,
            ]
        );
    }

    #[test]
    fn rejects_unknown_and_truncated_commands() {
        assert!(decode(&[0]).is_err());
        assert!(decode(&[15]).is_err());
        assert!(decode(&[-1]).is_err());
        assert!(decode(&[6, 1, 1]).is_err());
        assert_eq!(decode(&[]).unwrap(), vec![]);
        assert_eq!(
            decode(&[2, 3]).unwrap(),
            vec![Op::BeginComputePass, Op::EndPass]
        );
    }

    proptest! {
        #[test]
        fn arbitrary_streams_never_panic(words in proptest::collection::vec(any::<i32>(), 0..64)) {
            let _ = decode(&words);
        }

        #[test]
        fn small_opcode_streams_never_panic(
            words in proptest::collection::vec(-2i32..16, 0..64)
        ) {
            let _ = decode(&words);
        }
    }
}
