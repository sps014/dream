use super::*;

pub struct EntrySpec<'a> {
    pub name: &'a str,
    pub stage: &'a str,
    pub layout: u32,
    pub code: &'a [u8],
    pub thread_group_size: [u32; 3],
    pub params: Vec<Param>,
    pub capabilities: Vec<&'a str>,
}

pub struct TargetSpec<'a> {
    pub name: &'a str,
    pub code_format: &'a str,
    pub layouts: Vec<&'a [u8]>,
    pub entries: Vec<EntrySpec<'a>>,
}

pub fn bundle(targets: &[TargetSpec<'_>]) -> Vec<u8> {
    let mut data: Vec<u8> = Vec::new();
    let mut stored: Vec<(Vec<u8>, u32)> = Vec::new();
    let mut header: Vec<u8> = Vec::new();
    let u32s = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    let string = |out: &mut Vec<u8>, s: &str| {
        out.extend_from_slice(&(s.len() as u32).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    };
    let mut blob = |out: &mut Vec<u8>, raw: &[u8]| {
        let offset = match stored.iter().find(|(b, _)| b == raw) {
            Some((_, offset)) => *offset,
            None => {
                let offset = data.len() as u32;
                data.extend_from_slice(raw);
                stored.push((raw.to_vec(), offset));
                offset
            }
        };
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        out.extend_from_slice(&Sha256::digest(raw));
    };
    u32s(&mut header, targets.len() as u32);
    for target in targets {
        string(&mut header, target.name);
        string(&mut header, target.code_format);
        u32s(&mut header, target.layouts.len() as u32);
        for layout in &target.layouts {
            blob(&mut header, layout);
        }
        u32s(&mut header, target.entries.len() as u32);
        for entry in &target.entries {
            string(&mut header, entry.name);
            string(&mut header, entry.stage);
            u32s(&mut header, entry.layout);
            blob(&mut header, entry.code);
            for axis in entry.thread_group_size {
                u32s(&mut header, axis);
            }
            u32s(&mut header, entry.params.len() as u32);
            for p in &entry.params {
                string(&mut header, &p.name);
                for v in [p.kind as u32, p.flags, p.space, p.binding, p.offset, p.size] {
                    u32s(&mut header, v);
                }
                u32s(&mut header, p.webgpu_layout);
            }
            u32s(&mut header, entry.capabilities.len() as u32);
            for capability in &entry.capabilities {
                string(&mut header, capability);
            }
        }
    }
    let mut out = b"DGPB".to_vec();
    u32s(&mut out, 0);
    u32s(&mut out, (12 + header.len()) as u32);
    out.extend_from_slice(&header);
    out.extend_from_slice(&data);
    out
}
