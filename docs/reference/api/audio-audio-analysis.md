# AudioAnalysis

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## `AudioAnalysis`

A mono waveform and its single-sided frequency spectrum.

```dream
public class AudioAnalysis
```

## `waveform`

Mono samples before window weighting; short input is padded with silence.

```dream
public waveform: float[]
```

## `real`

Unnormalized real FFT components, including DC and Nyquist.

```dream
public real: float[]
```

## `imaginary`

Unnormalized imaginary FFT components, including DC and Nyquist.

```dream
public imaginary: float[]
```

## `magnitudes`

Window-compensated single-sided amplitudes; a unit sine has amplitude one.

```dream
public magnitudes: float[]
```

## `magnitudes_db`

Amplitudes in dBFS, floored at -120 dBFS.

```dream
public magnitudes_db: float[]
```

## `peak`

Largest absolute sample in the unweighted waveform.

```dream
public peak: float
```

## `rms`

Root mean square of the unweighted waveform, including zero padding.

```dream
public rms: float
```

## `frequency_hz`

Returns the frequency in Hz for a valid bin; invalid indices return EINVAL.

```dream
public fun frequency_hz(bin: int): Result<double, AudioError>
```
