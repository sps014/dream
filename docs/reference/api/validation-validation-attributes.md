# Validation Attributes

**Import:** `import system.validation;`

See [model validation](../../web-apis/validation.md) for strict decoding, constraints, defaults, and callbacks.

## `MinimumAttribute`

Inclusive lower numeric bound.

```dream
public struct MinimumAttribute
```

## `value`

Inclusive minimum numeric value.

```dream
public value: double
```

## `MaximumAttribute`

Inclusive upper numeric bound.

```dream
public struct MaximumAttribute
```

## `value`

Inclusive maximum numeric value.

```dream
public value: double
```

## `MinLengthAttribute`

Minimum string or collection length.

```dream
public struct MinLengthAttribute
```

## `value`

Minimum number of characters or collection elements.

```dream
public value: int
```

## `MaxLengthAttribute`

Maximum string or collection length.

```dream
public struct MaxLengthAttribute
```

## `value`

Maximum number of characters or collection elements.

```dream
public value: int
```

## `PatternAttribute`

Regular expression required for a string.

```dream
public struct PatternAttribute
```

## `value`

Regular expression that the input string must match.

```dream
public value: string
```

## `DefaultJsonAttribute`

JSON encoded field default, applied only when the property is absent.

```dream
public struct DefaultJsonAttribute
```

## `value`

JSON text decoded as the field default when the property is absent.

```dream
public value: string
```

## `AllowExtraAttribute`

Permits additional input properties on a model; the default is to reject them.

```dream
public struct AllowExtraAttribute
```

## `ValidateWithAttribute`

A synchronous field validator returning an optional error message.

```dream
public struct ValidateWithAttribute
```

## `validator`

Synchronous validation function; returns `None` on success or an error message on failure.

```dream
public validator: DeclarationPath
```

## `ModelValidatorAttribute`

A static model validator returning structured issues after fields pass validation.

```dream
public struct ModelValidatorAttribute
```
