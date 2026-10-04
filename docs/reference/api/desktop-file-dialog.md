# FileFilter, FileDialog

**Import:** `import system.desktop;`

Read the [usage guide](../stdlib/desktop.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class FileFilter`

One entry in a file dialog's type list, e.g. `Audio` -> `mp3`, `flac`.

```dream
public class FileFilter
```

## `name: string`

```dream
public name: string
```

## `extensions: List<string>`

```dream
public extensions: List<string>
```

## `constructor`

```dream
public constructor(name: string, extensions: List<string>)
```

## `class FileDialog`

Native open / save / folder picker. Set properties, then await one of the `pick_*` / `save_file` calls; cancelling yields `None` or an empty list.

```dream
public class FileDialog
```

## `constructor`

```dream
public constructor()
```

## `title`

```dream
public get title(): string
```

## `title`

```dream
public set title(value: string)
```

## `directory`

Folder the dialog starts in.

```dream
public get directory(): string
```

## `directory`

```dream
public set directory(value: string)
```

## `file_name`

Suggested name for `save_file`.

```dream
public get file_name(): string
```

## `file_name`

```dream
public set file_name(value: string)
```

## `parent`

`WebView.window_id` of the window that owns the dialog (a sheet on macOS); `-1` for none.

```dream
public get parent(): int
```

## `parent`

```dream
public set parent(value: int)
```

## `filters`

```dream
public get filters(): List<FileFilter>
```

## `add_filter`

Extensions without the dot: `add_filter("Audio", ["mp3", "flac"])`.

```dream
public fun add_filter(name: string, extensions: List<string>): void
```

## `pick_file`

```dream
public async fun pick_file(): Option<string>
```

## `pick_files`

```dream
public async fun pick_files(): List<string>
```

## `pick_folder`

```dream
public async fun pick_folder(): Option<string>
```

## `pick_folders`

```dream
public async fun pick_folders(): List<string>
```

## `save_file`

```dream
public async fun save_file(): Option<string>
```
