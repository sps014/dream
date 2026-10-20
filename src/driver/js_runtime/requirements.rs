use std::collections::BTreeSet;
use std::io::Error;

use crate::driver::abi::LiveImport;

#[derive(Clone, Copy)]
pub(super) struct Requirements {
    pub async_imports: bool,
    pub generic_imports: bool,
    pub async_main: bool,
    pub handles: bool,
    pub callbacks: bool,
    pub arrays: bool,
    pub workers: bool,
}

impl Requirements {
    #[cfg(test)]
    pub fn full(async_imports: bool) -> Self {
        Self {
            async_imports,
            generic_imports: true,
            async_main: true,
            handles: true,
            callbacks: true,
            arrays: true,
            workers: true,
        }
    }

    pub fn from_abi(abi: &serde_json::Value, imports: &[LiveImport]) -> Result<Self, Error> {
        let externs = abi["externs"]
            .as_array()
            .ok_or_else(|| Error::other("missing extern inventory"))?;
        let live: BTreeSet<_> = imports
            .iter()
            .map(|(module, field)| (module.as_str(), field.as_str()))
            .collect();
        let mut requirements = Self {
            async_imports: false,
            generic_imports: false,
            async_main: abi["main_async"] == true,
            handles: false,
            callbacks: false,
            arrays: false,
            workers: false,
        };
        for entry in externs {
            let module = entry["module"]
                .as_str()
                .ok_or_else(|| Error::other("extern has no module"))?;
            let field = entry["field"]
                .as_str()
                .ok_or_else(|| Error::other("extern has no field"))?;
            if !live.contains(&(module, field)) {
                continue;
            }
            requirements.async_imports |= entry["async"] == true;
            let params = entry["params"]
                .as_array()
                .ok_or_else(|| Error::other("extern has no parameter types"))?;
            let typed = params
                .iter()
                .chain(std::iter::once(&entry["result"]))
                .all(|ty| ty.as_str().is_some_and(super::typed::supports));
            for ty in params.iter().chain(std::iter::once(&entry["result"])) {
                let ty = ty
                    .as_str()
                    .ok_or_else(|| Error::other("extern type is not a string"))?;
                requirements.generic_imports |= !super::typed::supports(ty);
                requirements.arrays |= ty.contains("[]") && !typed;
                requirements.handles |= ty.contains("js");
                requirements.callbacks |= ty.contains("fun(");
            }
        }
        for (module, field) in imports {
            if module == "Dream" {
                if field.starts_with("js") {
                    requirements.generic_imports = true;
                    requirements.handles = true;
                    requirements.callbacks = true;
                    requirements.arrays = true;
                }
                requirements.workers |= dream_abi::runtime_hosts::is_worker_host_field(field);
            }
        }
        // Export consumers may inspect aggregate values through DreamInstance's public helpers.
        if abi["export_functions"]
            .as_array()
            .is_some_and(|exports| !exports.is_empty())
        {
            requirements.handles = true;
            requirements.callbacks = true;
            requirements.arrays = true;
        }
        if requirements.workers {
            requirements.callbacks = true;
        }
        Ok(requirements)
    }

    pub fn constants(self) -> String {
        format!(
            "const HAS_ASYNC_MAIN = {};\nconst HAS_GENERIC_IMPORTS = {};\nconst HAS_JS_HANDLES = {};\nconst HAS_CALLBACKS = {};\nconst HAS_ARRAYS = {};\nconst HAS_WORKERS = {};\n",
            self.async_main,
            self.generic_imports,
            self.handles,
            self.callbacks,
            self.arrays,
            self.workers
        )
    }

    pub fn select_instance(self, text: &str) -> Result<String, Error> {
        use oxc_ast::ast::{ClassElement, Statement};
        let allocator = oxc_allocator::Allocator::default();
        let parsed = oxc_parser::Parser::new(&allocator, text, oxc_span::SourceType::mjs()).parse();
        if let Some(error) = parsed.diagnostics.first() {
            return Err(Error::other(error.to_string()));
        }
        let mut omitted = BTreeSet::new();
        if !self.handles {
            omitted.extend([
                "registerHandle",
                "derefHandle",
                "retainValue",
                "releaseValue",
            ]);
        }
        if !self.callbacks {
            omitted.insert("callback");
        }
        if !self.arrays {
            omitted.extend([
                "_readElement",
                "_writeElement",
                "readArray",
                "writeArray",
                "readList",
                "readStruct",
            ]);
        }
        if !self.workers {
            omitted.extend([
                "__workerInvoke",
                "__workerReset",
                "__workerClose",
                "__awaitWorkerResult",
            ]);
        }
        if !self.async_main && !self.workers {
            omitted.extend(["__valueTag", "__isFutureFrame", "__awaitFuture"]);
        }
        let mut spans = Vec::new();
        for statement in &parsed.program.body {
            let Statement::ClassDeclaration(class) = statement else {
                continue;
            };
            if class
                .id
                .as_ref()
                .is_none_or(|id| id.name != "DreamInstance")
            {
                continue;
            }
            for element in &class.body.body {
                if let ClassElement::MethodDefinition(method) = element
                    && method
                        .key
                        .static_name()
                        .is_some_and(|name| omitted.contains(name.as_ref()))
                {
                    spans.push(method.span);
                }
            }
        }
        let mut selected = String::new();
        let mut cursor = 0;
        for span in spans {
            selected.push_str(&text[cursor..span.start as usize]);
            cursor = span.end as usize;
        }
        selected.push_str(&text[cursor..]);
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_live_signatures_select_marshaling_features() {
        let abi = json!({"externs": [
            {"module": "Dream", "field": "delayMs", "params": ["int"], "result": "void", "async": true},
            {"module": "app", "field": "unused", "params": ["fun(js):byte[]"], "result": "js", "async": true},
            {"module": "app", "field": "receive", "params": ["byte[]"], "result": "bool", "async": false}
        ]});
        let live = vec![("Dream".into(), "delayMs".into())];
        let requirements = Requirements::from_abi(&abi, &live).unwrap();
        assert!(requirements.async_imports);
        assert!(!requirements.callbacks);
        assert!(!requirements.handles);
        assert!(!requirements.arrays);
        assert!(!requirements.workers);
        let live = vec![("app".into(), "receive".into())];
        let requirements = Requirements::from_abi(&abi, &live).unwrap();
        assert!(!requirements.arrays);
        assert!(!requirements.generic_imports);
        assert!(!requirements.async_imports);
        assert!(!requirements.callbacks);
    }

    #[test]
    fn dynamic_interop_exports_and_workers_keep_their_instance_helpers() {
        let abi = json!({"externs": []});
        let live = vec![("Dream".into(), "jsCall".into())];
        let requirements = Requirements::from_abi(&abi, &live).unwrap();
        assert!(requirements.callbacks && requirements.handles && requirements.arrays);
        let live = vec![("Dream".into(), "workerSpawn".into())];
        let requirements = Requirements::from_abi(&abi, &live).unwrap();
        assert!(requirements.workers && requirements.callbacks);
        let abi = json!({"externs": [], "export_functions": [{"name":"inspect"}]});
        let requirements = Requirements::from_abi(&abi, &[]).unwrap();
        assert!(requirements.callbacks && requirements.handles && requirements.arrays);
    }

    #[test]
    fn mixed_callback_signatures_keep_generic_array_helpers() {
        let abi = json!({"externs": [{"module": "app", "field": "receive",
            "params": ["byte[]", "fun(int):int"], "result": "void", "async": false}]});
        let live = vec![("app".into(), "receive".into())];
        let requirements = Requirements::from_abi(&abi, &live).unwrap();
        assert!(requirements.generic_imports && requirements.callbacks && requirements.arrays);
    }
}
