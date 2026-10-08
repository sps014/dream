//! Content-verified C units, including clang-recorded transitive and system headers.
use crate::driver::rt_stamp;
use crate::driver::wasi::run_captured;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Deserialize)]
struct Entry {
    dependencies: Vec<PathBuf>,
    fingerprint: String,
    object: String,
}

#[derive(Serialize, Deserialize)]
struct Namespace {
    directories: Vec<PathBuf>,
    stamp: String,
    digest: String,
}

pub(super) type IncludeInventory =
    std::sync::Mutex<std::collections::BTreeMap<PathBuf, Option<String>>>;

fn inventory_namespace(path: &Path, root: &Path, inventory: &IncludeInventory) -> Option<String> {
    inventory
        .lock()
        .ok()?
        .entry(path.to_path_buf())
        .or_insert_with(|| include_namespace(path, Some(root)))
        .clone()
}

pub(super) struct CompiledUnit {
    pub dependencies: Vec<PathBuf>,
    pub object: PathBuf,
}

fn completed_unit(
    cache: &Path,
    object: &Path,
    output: &Path,
    dependencies: Vec<PathBuf>,
) -> Result<CompiledUnit, String> {
    let typed = object.with_extension(
        output
            .extension()
            .ok_or("runtime unit omitted its artifact type")?,
    );
    std::fs::copy(object, &typed).map_err(|error| error.to_string())?;
    let immutable = super::runtime_snapshot::publish(cache, &[typed])?
        .pop()
        .ok_or("runtime snapshot omitted its object")?;
    std::fs::copy(&immutable, output).map_err(|error| error.to_string())?;
    Ok(CompiledUnit {
        dependencies,
        object: immutable,
    })
}

fn tracked_include(path: &Path) -> bool {
    path.extension().is_none_or(|extension| {
        matches!(
            extension.to_str(),
            Some(
                "c" | "s" | "S" | "h" | "hpp" | "hxx" | "inc" | "def" | "tcc" | "inl" | "modulemap"
            )
        )
    })
}

fn include_namespace(path: &Path, cache: Option<&Path>) -> Option<String> {
    static NAMESPACES: std::sync::OnceLock<
        std::sync::Mutex<std::collections::BTreeMap<PathBuf, Namespace>>,
    > = std::sync::OnceLock::new();
    let mut namespaces = NAMESPACES.get_or_init(Default::default).lock().unwrap();
    if let Some(entry) = namespaces.get(path)
        && entry.stamp == rt_stamp::fingerprint(entry.directories.clone())
    {
        return Some(entry.digest.clone());
    }
    let persisted = cache.map(|root| {
        root.join("include-inventories").join(format!(
            "{}.namespace.json",
            blake3::hash(path.as_os_str().as_encoded_bytes()).to_hex()
        ))
    });
    if let Some(file) = &persisted
        && let Ok(bytes) = std::fs::read(file)
        && std::fs::read_to_string(file.with_extension("integrity"))
            .is_ok_and(|hash| hash == blake3::hash(&bytes).to_hex().as_str())
        && let Ok(entry) = serde_json::from_slice::<Namespace>(&bytes)
        && entry.directories.first().is_some_and(|root| root == path)
        && entry.stamp == rt_stamp::fingerprint(entry.directories.clone())
    {
        let digest = entry.digest.clone();
        namespaces.insert(path.to_path_buf(), entry);
        return Some(digest);
    }
    let mut files = Vec::new();
    let mut directories = vec![path.to_path_buf()];
    let mut pending = directories.clone();
    let mut visited = std::collections::BTreeSet::new();
    while let Some(dir) = pending.pop() {
        if dir.exists() && !visited.insert(std::fs::canonicalize(&dir).ok()?) {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            let mut entries = entries.collect::<Result<Vec<_>, _>>().ok()?;
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let path = entry.path();
                let kind = entry.file_type().ok()?;
                if kind.is_dir() || (kind.is_symlink() && path.is_dir()) {
                    directories.push(path.clone());
                    pending.push(path);
                } else if tracked_include(&path) {
                    files.push(path);
                }
            }
        } else if dir.exists() {
            return None;
        }
    }
    files.sort();
    let mut hash = blake3::Hasher::new();
    for path in files {
        let bytes = path.as_os_str().as_encoded_bytes();
        hash.update(&(bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    let digest = hash.finalize().to_hex().to_string();
    let stamp = rt_stamp::fingerprint(directories.clone());
    let entry = Namespace {
        directories,
        stamp,
        digest: digest.clone(),
    };
    if let Some(file) = persisted
        && let Some(parent) = file.parent()
        && std::fs::create_dir_all(parent).is_ok()
        && let Ok(bytes) = serde_json::to_vec(&entry)
    {
        let partial = file.with_extension(format!("partial-{}", std::process::id()));
        if std::fs::write(&partial, &bytes).is_ok() && std::fs::rename(&partial, &file).is_ok() {
            let _ = std::fs::write(
                file.with_extension("integrity"),
                blake3::hash(&bytes).to_hex().as_str(),
            );
        }
    }
    namespaces.insert(path.to_path_buf(), entry);
    Some(digest)
}

pub(super) fn compile(
    mut command: Command,
    root: &Path,
    output: &Path,
    cache_allowed: bool,
    namespaces: &IncludeInventory,
) -> Result<CompiledUnit, String> {
    for name in [
        "CPATH",
        "C_INCLUDE_PATH",
        "CPLUS_INCLUDE_PATH",
        "OBJC_INCLUDE_PATH",
    ] {
        let value = command
            .get_envs()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.map(std::ffi::OsStr::to_os_string))
            .unwrap_or_else(|| std::env::var_os(name));
        if let Some(value) = value {
            command.env(name, value);
        }
    }
    let cache = root.join("units");
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let args: Vec<_> = command.get_args().collect();
    let mut include_inventory = std::collections::BTreeMap::new();
    for source in args.iter().map(Path::new).filter(|path| {
        path.is_file()
            && path
                .extension()
                .is_some_and(|ext| ext == "c" || ext == "s" || ext == "S")
    }) {
        if let Some(parent) = source.parent() {
            include_inventory.insert(
                parent.to_path_buf(),
                inventory_namespace(parent, root, namespaces),
            );
        }
    }
    for (name, value) in command.get_envs() {
        if matches!(
            name.to_str(),
            Some("CPATH" | "C_INCLUDE_PATH" | "CPLUS_INCLUDE_PATH" | "OBJC_INCLUDE_PATH")
        ) && let Some(value) = value
        {
            for path in std::env::split_paths(value) {
                let path = if path.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    path
                };
                include_inventory
                    .insert(path.clone(), inventory_namespace(&path, root, namespaces));
            }
        }
    }
    for (index, arg) in args.iter().enumerate() {
        let Some(arg) = arg.to_str() else {
            continue;
        };
        let path = if matches!(
            arg,
            "-I" | "-isystem" | "-iquote" | "-isysroot" | "--sysroot"
        ) {
            args.get(index + 1).map(Path::new)
        } else {
            arg.strip_prefix("-I")
                .or_else(|| arg.strip_prefix("--sysroot="))
                .map(Path::new)
        };
        if let Some(path) = path {
            include_inventory.insert(
                path.to_path_buf(),
                inventory_namespace(path, root, namespaces),
            );
        }
    }
    let program = PathBuf::from(command.get_program());
    let executable = if program.components().count() == 1 {
        let search = command
            .get_envs()
            .find(|(name, _)| *name == "PATH")
            .map(|(_, value)| value.map(std::ffi::OsStr::to_os_string))
            .unwrap_or_else(|| std::env::var_os("PATH"));
        search
            .and_then(|search| {
                std::env::split_paths(&search)
                    .map(|dir| dir.join(&program))
                    .find(|path| path.is_file())
            })
            .ok_or("runtime compiler is not on PATH")?
    } else if program.is_absolute() {
        program
    } else {
        command
            .get_current_dir()
            .map(Path::to_path_buf)
            .unwrap_or(std::env::current_dir().map_err(|e| e.to_string())?)
            .join(program)
    };
    let identity = format!(
        "{:?}:{}",
        include_inventory,
        rt_stamp::tool_identity(&executable).ok_or("runtime compiler identity is unreadable")?
    );
    let cacheable = include_inventory.values().all(Option::is_some) && cache_allowed;
    let key = blake3::hash(format!("unit-v2:{identity}:{command:?}").as_bytes())
        .to_hex()
        .to_string();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(cache.join(format!("{key}.lock")))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    let stamp = cache.join(format!("{key}.json"));
    let object = cache.join(format!("{key}.object"));
    if cacheable
        && let Ok(bytes) = std::fs::read(&stamp)
        && let Ok(entry) = serde_json::from_slice::<Entry>(&bytes)
        && entry.dependencies.iter().all(|path| path.is_file())
        && entry.dependencies.iter().all(|path| tracked_include(path))
        && entry.fingerprint == rt_stamp::content_fingerprint(entry.dependencies.clone())
        && rt_stamp::content_hash(&object)
            .is_some_and(|hash| hash.to_hex().as_str() == entry.object)
    {
        return completed_unit(&cache, &object, output, entry.dependencies);
    }
    let partial = cache.join(format!("{key}.partial"));
    let deps = cache.join(format!("{key}.d"));
    command
        .args(["-MD", "-MF"])
        .arg(&deps)
        .args(["-MT", "dream-unit"])
        .arg("-o")
        .arg(&partial);
    run_captured(&mut command, "clang (runtime unit)")?;
    let text = std::fs::read_to_string(&deps).map_err(|e| e.to_string())?;
    let text = if cfg!(windows) { normalize_dependency_separators(&text) } else { text };
    let parsed = depfile::parse(&text).map_err(|e| format!("clang dependency file: {e:?}"))?;
    let dependencies: Vec<PathBuf> = parsed
        .find("dream-unit")
        .ok_or("clang omitted dependency target")?
        .iter()
        .map(|path| PathBuf::from(path.as_ref()))
        .collect();
    if dependencies.is_empty() {
        return Err("clang omitted runtime dependencies".into());
    }
    let entry = Entry {
        fingerprint: rt_stamp::content_fingerprint(dependencies.clone()),
        dependencies: dependencies.clone(),
        object: rt_stamp::content_hash(&partial)
            .ok_or("missing runtime unit")?
            .to_hex()
            .to_string(),
    };
    std::fs::File::open(&partial)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    std::fs::rename(&partial, &object).map_err(|e| e.to_string())?;
    let manifest = stamp.with_extension("partial.json");
    std::fs::write(
        &manifest,
        serde_json::to_vec(&entry).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(manifest, stamp).map_err(|e| e.to_string())?;
    completed_unit(&cache, &object, output, dependencies)
}

fn normalize_dependency_separators(text: &str) -> String {
    // Clang emits literal Windows separators, which the Make parser treats as escapes.
    let mut chars = text.chars().peekable();
    let mut normalized = String::with_capacity(text.len());
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek().copied() {
                Some('\\' | ' ' | '\t' | '\r' | '\n' | ':' | '#' | '$') => {
                    normalized.push(ch);
                    normalized.push(chars.next().unwrap());
                    continue;
                }
                Some(_) => { normalized.push('/'); continue; }
                None => {}
            }
        }
        normalized.push(ch);
    }
    normalized
}

#[cfg(test)]
mod dependency_tests {
    use super::*;

    #[test]
    fn clang_windows_paths_keep_spaces_and_continuations() {
        let text = "dream-unit: D:\\a\\dream\\core.c \\\r\n C:\\Program\\ Files\\sdk.h\r\n";
        let normalized = normalize_dependency_separators(text);
        let parsed = depfile::parse(&normalized).unwrap();
        let paths: Vec<_> = parsed.find("dream-unit").unwrap().iter().map(|s| s.as_ref()).collect();
        assert_eq!(paths, ["D:/a/dream/core.c", "C:/Program Files/sdk.h"]);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn include_inventory_tracks_symlinked_directories_without_recursing_forever() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let headers = root.join("headers");
        std::fs::create_dir(&headers).unwrap();
        std::os::unix::fs::symlink(&headers, root.join("alias")).unwrap();
        std::os::unix::fs::symlink(root, headers.join("cycle")).unwrap();
        let first = include_namespace(root, None).unwrap();
        std::fs::write(headers.join("new.h"), "#define ADDED 1\n").unwrap();
        assert_ne!(first, include_namespace(root, None).unwrap());
    }

    #[test]
    fn replacing_include_directory_with_preserved_mtime_invalidates_inventory() {
        let temporary = tempfile::tempdir().unwrap();
        let first = temporary.path().join("first");
        let second = temporary.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        std::fs::write(first.join("a.h"), "").unwrap();
        std::fs::write(second.join("b.h"), "").unwrap();
        let modified = std::fs::metadata(&first).unwrap().modified().unwrap();
        std::fs::File::open(&second)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        let alias = temporary.path().join("alias");
        std::os::unix::fs::symlink(&first, &alias).unwrap();
        let inventory = include_namespace(&alias, None).unwrap();
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(&second, &alias).unwrap();
        assert_ne!(inventory, include_namespace(&alias, None).unwrap());
    }

    #[test]
    fn concurrent_writers_publish_one_complete_unit() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("concurrent.c");
        std::fs::write(&source, "int answer(void) { return 42; }\n").unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        std::thread::scope(|scope| {
            let mut threads = Vec::new();
            for i in 0..4 {
                let barrier = barrier.clone();
                let source = &source;
                let root = temp.path();
                threads.push(scope.spawn(move || {
                    let output = root.join(format!("out-{i}.o"));
                    let mut command = Command::new("cc");
                    command.arg("-c").arg(source);
                    barrier.wait();
                    compile(command, root, &output, true, &Default::default()).unwrap();
                    std::fs::read(output).unwrap()
                }));
            }
            let outputs: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
            assert!(outputs.windows(2).all(|pair| pair[0] == pair[1]));
        });
        assert_eq!(
            std::fs::read_dir(temp.path().join("units"))
                .unwrap()
                .filter(|entry| entry
                    .as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == "object"))
                .count(),
            1
        );
    }

    #[test]
    fn transitive_headers_and_corrupt_objects_invalidate_units() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let source = root.join("unit.c");
        let header = root.join("nested.h");
        let indirect = root.join("outer.h");
        std::fs::write(
            &source,
            "#include \"outer.h\"\nint answer(void) { return ANSWER; }\n",
        )
        .unwrap();
        std::fs::write(&indirect, "#include \"nested.h\"\n").unwrap();
        std::fs::write(&header, "#define ANSWER 1\n").unwrap();
        let output = root.join("output.o");
        let build = || {
            let mut command = Command::new("cc");
            command.arg("-c").arg(&source);
            compile(command, root, &output, true, &Default::default())
                .unwrap()
                .dependencies
        };
        assert!(build().contains(&header));
        let first = std::fs::read(&output).unwrap();
        let object = std::fs::read_dir(root.join("units"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.extension().is_some_and(|ext| ext == "object"))
            .unwrap();
        let modified = std::fs::metadata(&object).unwrap().modified().unwrap();
        build();
        assert_eq!(
            modified,
            std::fs::metadata(&object).unwrap().modified().unwrap()
        );
        std::fs::write(&header, "#define ANSWER 2\n").unwrap();
        build();
        let second = std::fs::read(&output).unwrap();
        assert_ne!(first, second);
        std::fs::write(&object, "corrupt").unwrap();
        build();
        assert_eq!(second, std::fs::read(&output).unwrap());
    }
}
