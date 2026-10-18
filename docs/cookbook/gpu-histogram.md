---
title: Count values on the GPU
summary: Build a histogram of 100,000 scores with a compute shader and atomic counters.
icon: material/chart-bar
---

# Count values on the GPU

This program generates 100,000 test scores from 0 to 100 and counts how many fall into each band
of ten on the GPU. Every GPU thread reads one score and adds one to its band's counter. The
counters use `GpuAtomic.add`, so threads that update the same band at the same moment do not lose
counts.

Building it needs the shader compiler, which you install once with
`dreamer toolchain install slang`.

```dream
import system;
import system.gpu;

@compute(256, 1, 1)
fun count_scores(@readonly scores: GpuBuffer<int>, bins: GpuBuffer<int>, n: int): void {
    let i = global_id().x;
    if i < n {
        let bin = GpuMath.min(scores[i] / 10, 9);
        GpuAtomic.add(bins, bin, 1);
    }
}

async fun histogram(scores: int[]): Result<int[], GpuError> {
    let device = GpuDevice.create(GpuBackend.Auto).await?;
    let input = device.buffer_from(scores)?;
    let bins = device.buffer_alloc<int>(10)?;

    GpuShaders.count_scores.dispatch(
        device,
        GpuDispatch.threads_1d(scores.length),
        input,
        bins,
        scores.length
    ).await?;
    return bins.read().await;
}

async fun main(): void {
    let random = Random(42u);
    let scores = Array.alloc<int>(100000);
    for (let i = 0; i < scores.length; i = i + 1) {
        scores[i] = (random.next_int(101) + random.next_int(101)) / 2;
    }

    switch (histogram(scores).await) {
        Ok(bins) => {
            let total = 0;
            for (let b = 0; b < bins.length; b = b + 1) {
                let low = b * 10;
                let bar = "#".repeat(bins[b] / 1000);
                System.println(low.to_string().pad_start(3, " ") + "+ " + bar);
                total = total + bins[b];
            }
            System.println("counted " + total.to_string() + " scores");
        }
        Err(e) => {
            System.println("GPU error: " + e.message());
        }
    }
}
```

Run it with `dream run main.dream`:

```text
  0+ ##
 10+ #####
 20+ ##########
 30+ #############
 40+ #################
 50+ #################
 60+ #############
 70+ ##########
 80+ ######
 90+ ##
counted 100000 scores
```

Each `#` stands for 1,000 scores. Averaging two random numbers makes middle scores more common, so
the bars peak around 50. Scores of exactly 100 go into the last band, because
`GpuMath.min(scores[i] / 10, 9)` caps the band index.

How it works:

- `@compute(256, 1, 1)` runs the shader in groups of 256 threads, and
  `GpuDispatch.threads_1d(scores.length)` starts enough groups for one thread per score. The
  last group is only partly needed, so the shader checks `i < n`.
- `buffer_alloc<int>(10)` creates ten counters that start at zero.
- `?` returns the first `GpuError`, for example when the machine has no GPU, and `main` prints it.

Things to try:

- Change the band width to 5 and the number of bins to 20.
- Replace the scores with the bytes of an image file to count brightness levels.

[GPU programming](../library/gpu/index.md) explains devices, buffers and dispatch sizes in more
detail.
