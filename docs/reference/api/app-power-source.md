# Power Source

**Import:** `import system.app;`

Read the [usage guide](../../library/system/app.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `PowerSource`

Where the computer gets its power from, as reported by `App.power_source`.

```dream
public enum PowerSource
```

## `Ac`

The computer is plugged in to wall power.

```dream
Ac
```

## `Battery`

The computer is running on its battery or a UPS.

```dream
Battery
```

## `Unknown`

The platform does not report a power source, as in browsers, Node, and Linux.

```dream
Unknown
```
