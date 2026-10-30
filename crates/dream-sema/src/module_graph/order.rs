use super::{ModuleId, SourceFile, SourceModule};
use indexmap::IndexMap;

pub(super) fn dependencies(
    files: &[SourceFile<'_>],
    modules: &[SourceModule],
    edges: &IndexMap<String, Vec<String>>,
) -> Vec<Vec<ModuleId>> {
    let names: IndexMap<_, _> = modules.iter().map(|m| (m.path.as_str(), m.id)).collect();
    let paths: IndexMap<_, _> = files.iter().map(|f| (f.path.as_str(), f.module)).collect();
    let mut dependencies = vec![Vec::new(); modules.len()];
    for file in files {
        let imports = &mut dependencies[file.module.0 as usize];
        for import in file
            .program
            .imports
            .iter()
            .filter(|import| import.alias.is_some())
        {
            if let Some((path, _)) = import.module_name.text.rsplit_once('.')
                && let Some(&id) = names.get(path)
                && id != file.module
                && !imports.contains(&id)
            {
                imports.push(id);
            }
        }
        for target in edges.get(&file.path).into_iter().flatten() {
            if let Some(&id) = paths.get(target.as_str())
                && id != file.module
                && !imports.contains(&id)
            {
                imports.push(id);
            }
        }
    }
    dependencies
}

pub(super) fn modules(dependencies: &[Vec<ModuleId>]) -> Vec<ModuleId> {
    let mut order = Vec::with_capacity(dependencies.len());
    let mut visited = vec![false; dependencies.len()];
    for root in 0..dependencies.len() {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut stack = vec![(ModuleId(root as u32), 0usize)];
        while let Some((id, next)) = stack.last_mut() {
            if let Some(&import) = dependencies[id.0 as usize].get(*next) {
                *next += 1;
                if !visited[import.0 as usize] {
                    visited[import.0 as usize] = true;
                    stack.push((import, 0));
                }
            } else {
                order.push(*id);
                stack.pop();
            }
        }
    }
    order
}
