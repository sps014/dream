# AudioDevice

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## `AudioDevice`

An output device visible to the current application.

```dream
public class AudioDevice
```

## `id`

Opaque host-specific ID; the empty ID selects the system default.

```dream
public id: string
```

## `name`

User-visible device label, possibly empty before browser permission.

```dream
public name: string
```

## `is_default`

Whether this is currently the system default output.

```dream
public is_default: bool
```
