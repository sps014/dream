use super::*;
use dream_drawing::Picture;
use dream_host_gpu::{devices, resources, testing};
use std::sync::{Mutex, mpsc};

static COMPLETIONS: Mutex<Option<mpsc::Sender<(usize, u64)>>> = Mutex::new(None);
fn complete(future: usize, value: u64) {
    if let Some(sender) = COMPLETIONS.lock().unwrap().as_ref() {
        sender.send((future, value)).unwrap();
    }
}

#[test]
#[ignore = "requires a real D3D12 GPU; absence is a failure"]
fn dx12_drawing_copies_without_readback_and_preserves_bytes() {
    let (sender, receiver) = mpsc::channel();
    *COMPLETIONS.lock().unwrap() = Some(sender);
    testing::set_completion_hook(complete);
    assert!(
        COPY.set(Some(dream_host_gpu::drawing::dream_gpu_drawing_copy_v1))
            .is_ok()
    );
    let device = devices::create_now(1, 0).unwrap();
    let mut surface = pollster::block_on(Surface::create(16, 16, 2)).unwrap();
    surface
        .render(&Picture::decode(&[0., 40. / 255., 100. / 255., 200. / 255., 0.5]).unwrap())
        .unwrap();
    let handle = dream_host_abi::lock(&SURFACES).insert(surface).unwrap();
    for format in 0..4 {
        let texture = resources::texture_create(device, &[16, 16, 1, 1, format, 0, 1]).unwrap();
        for frame in 0..3 {
            dream_host_abi::lock(&SURFACES)
                .get_mut(handle)
                .unwrap()
                .render(
                    &Picture::decode(&[
                        0.,
                        (40. + 20. * frame as f32) / 255.,
                        100. / 255.,
                        200. / 255.,
                        0.5,
                    ])
                    .unwrap(),
                )
                .unwrap();
            assert_eq!(
                copy(handle, texture, 16, 16),
                1,
                "{}",
                super::super::ERROR.with(|e| e.borrow().clone())
            );
            assert_eq!(
                dream_host_abi::lock(&SURFACES)
                    .get(handle)
                    .unwrap()
                    .stats()
                    .pixel_reads,
                0
            );
            resources::texture_read(1, texture, 0, 0);
            let (_, value) = receiver
                .recv_timeout(std::time::Duration::from_secs(20))
                .unwrap();
            let pixels = unsafe { dream_host_gpu::guest::test_bytes(value) };
            let expected = if format >= 2 {
                [100, 50, 20 + 10 * frame, 128]
            } else {
                [20 + 10 * frame, 50, 100, 128]
            };
            assert_eq!(pixels.len(), 16 * 16 * 4, "frame {frame}");
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| *pixel == expected),
                "format {format}: {:?}",
                &pixels[..4]
            );
        }
        testing::gpuRelease(texture);
    }
    assert_eq!(
        testing::drawing_allocations(device).unwrap(),
        2,
        "sRGB variants share the same transfer texture"
    );
    let wrong = resources::texture_create(device, &[8, 16, 1, 1, 0, 0, 1]).unwrap();
    assert_eq!(copy(handle, wrong, 16, 16), 0);
    testing::gpuRelease(wrong);
    assert_eq!(
        copy(handle, wrong, 16, 16),
        0,
        "released targets are errors"
    );
    let array = resources::texture_create(device, &[16, 16, 2, 1, 0, 0, 1]).unwrap();
    assert_eq!(copy(handle, array, 16, 16), 1);
    resources::texture_read(1, array, 0, 0);
    let (_, value) = receiver
        .recv_timeout(std::time::Duration::from_secs(20))
        .unwrap();
    assert_eq!(
        &unsafe { dream_host_gpu::guest::test_bytes(value) }[..4],
        &[40, 50, 100, 128]
    );
    testing::gpuRelease(array);
    // Resource handles outlive the guest's device handle; copying still orders on its queue.
    let target = resources::texture_create(device, &[16, 16, 1, 1, 0, 0, 1]).unwrap();
    testing::gpuRelease(device);
    assert_eq!(copy(handle, target, 16, 16), 1);
    dream_host_abi::lock(&SURFACES).remove(handle);
    resources::texture_read(1, target, 0, 0);
    let (_, value) = receiver
        .recv_timeout(std::time::Duration::from_secs(20))
        .unwrap();
    assert_eq!(
        &unsafe { dream_host_gpu::guest::test_bytes(value) }[..4],
        &[40, 50, 100, 128]
    );
    testing::gpuRelease(target);
    if std::env::var_os("DREAM_DRAWING_BENCH").is_some() {
        benchmark(&receiver);
    }
}

fn benchmark(receiver: &mpsc::Receiver<(usize, u64)>) {
    let device = devices::create_now(1, 0).unwrap();
    let target = resources::texture_create(device, &[320, 240, 1, 1, 0, 0, 1]).unwrap();
    let mut surface = pollster::block_on(Surface::create(320, 240, 2)).unwrap();
    surface
        .render(&Picture::decode(&[0., 0.3, 0.6, 0.9, 1.]).unwrap())
        .unwrap();
    let handle = dream_host_abi::lock(&SURFACES).insert(surface).unwrap();
    assert_eq!(copy(handle, target, 320, 240), 1);
    let allocations = testing::drawing_allocations(device).unwrap();
    let start = std::time::Instant::now();
    for _ in 0..30 {
        assert_eq!(copy(handle, target, 320, 240), 1);
    }
    let direct = start.elapsed().as_secs_f64() * 1e6 / 30.;
    assert_eq!(testing::drawing_allocations(device).unwrap(), allocations);
    assert_eq!(
        dream_host_abi::lock(&SURFACES)
            .get(handle)
            .unwrap()
            .stats()
            .pixel_reads,
        0
    );
    resources::texture_read(1, target, 0, 0);
    let (_, value) = receiver
        .recv_timeout(std::time::Duration::from_secs(20))
        .unwrap();
    let direct_pixels = unsafe { dream_host_gpu::guest::test_bytes(value) };
    let start = std::time::Instant::now();
    for _ in 0..30 {
        let pixels = pollster::block_on(
            dream_host_abi::lock(&SURFACES)
                .get(handle)
                .unwrap()
                .pixels(),
        )
        .unwrap();
        resources::texture_write(target, 0, 0, &pixels).unwrap();
    }
    let old = start.elapsed().as_secs_f64() * 1e6 / 30.;
    resources::texture_read(1, target, 0, 0);
    let (_, value) = receiver
        .recv_timeout(std::time::Duration::from_secs(20))
        .unwrap();
    assert_eq!(direct_pixels, unsafe {
        dream_host_gpu::guest::test_bytes(value)
    });
    println!(
        "native_copy_benchmark: legacy_wall_us={old:.1} direct_submission_us={direct:.1} shared_targets={allocations} warm_new_targets=0 direct_pixel_reads=0"
    );
    dream_host_abi::lock(&SURFACES).remove(handle);
    testing::gpuRelease(target);
    testing::gpuRelease(device);
}
