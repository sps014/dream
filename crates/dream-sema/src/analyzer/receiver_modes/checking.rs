use super::*;

impl<'a> Analyzer<'a> {
    pub(super) fn check_receiver_contracts(
        &mut self,
        node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
        registry: &indexmap::IndexMap<MethodKey, Entry>,
        resolved_edges: &indexmap::IndexMap<MethodKey, Vec<(MethodKey, TextSpan)>>,
    ) {
        // --- Diagnostics + storage ----------------------------------------------------------
        for (key, e) in registry {
            // A pinned `borrow` whose body still resolves to mutation is a contract violation.
            // Note: explicit-Borrow entries never get `inferred_unique`, so test the raw facts
            // plus whether any edge reaches a unique method.
            if e.explicit == Some(ReceiverMode::Borrow)
                && (e.direct_unique
                    || resolved_edges[key]
                        .iter()
                        .any(|(t, _)| registry[t].is_unique()))
            {
                // Only override the bag's current file when the owner's own file is known;
                // otherwise the diagnostic renders against a stale path.
                let file = file_for_owner(node, &e.owner);
                if file.is_some() {
                    diagnostics.file_path = file_path_string(&file);
                }
                diagnostics.report_error(
                    format!(
                        "method '{}.{}' is declared 'borrow' but its body mutates the instance — a 'borrow' method promises not to mutate '{}'. Change the declaration to 'unique fun', or move the mutation out",
                        e.owner, e.name, e.owner
                    ),
                    Some(e.first_mutate_span.unwrap_or(e.decl_span)),
                );
            }
            self.receiver_modes.insert(key.clone(), e.effective_mode());
        }

        // --- Conformance: implementor modes must match interface contracts ------------------
        // A class implementing `I` must declare each of I's methods with the same receiver
        // mode the interface declares (signature-only methods default to Borrow). Methods the
        // class omits inherit an interface default body, whose mode is by definition the
        // interface's own — nothing to check.
        for s in node.structs.iter() {
            for iface_ty in &s.implements {
                let iface_name = match iface_ty {
                    Type::Struct(token, _) => token.text.clone(),
                    _ => continue,
                };
                let mut required: Vec<(String, ReceiverMode)> = Vec::new();
                self.collect_interface_contract(node, &iface_name, &mut required, &mut Vec::new());
                if required.is_empty() {
                    continue;
                }
                for (mname, imode) in required {
                    let own = s
                        .methods
                        .iter()
                        .find(|m| !m.is_static && m.name.text == mname);
                    let Some(own) = own else { continue };
                    let impl_key = format!("{}::{mname}", s.name.text);
                    let Some(e) = registry.get(&impl_key) else {
                        continue;
                    };
                    let impl_mode = e.effective_mode();
                    if impl_mode != imode {
                        let file = file_for_owner(node, &e.owner);
                        if file.is_some() {
                            diagnostics.file_path = file_path_string(&file);
                        }
                        diagnostics.report_error(
                            format!(
                                "'{}.{}' is inferred {} but interface '{}' declares it {} — a caller holding the interface value must be able to rely on the same mutation rights. Align the two (mark the interface 'unique' if implementing classes mutate '{}', or make this method read-only)",
                                s.name.text,
                                mname,
                                match impl_mode { ReceiverMode::Unique => "unique", ReceiverMode::Borrow => "borrow" },
                                iface_name,
                                match imode { ReceiverMode::Unique => "'unique'", ReceiverMode::Borrow => "'borrow'" },
                                s.name.text,
                            ),
                            Some(own.name.position),
                        );
                    }
                }
            }
        }
    }

    /// Collects `(method name, declared mode)` for `iface_name` and every transitive parent.
    /// `visited` guards inheritance cycles.
    pub(super) fn collect_interface_contract(
        &self,
        node: &'a ProgramView<'a>,
        iface_name: &str,
        out: &mut Vec<(String, ReceiverMode)>,
        visited: &mut Vec<String>,
    ) {
        if visited.iter().any(|v| v == iface_name) {
            return;
        }
        visited.push(iface_name.to_string());
        if let Some(parents) = self.interface_parent_types(iface_name) {
            for parent in parents {
                if let Type::Struct(tok, _) = parent {
                    self.collect_interface_contract(node, &tok.text, out, visited);
                }
            }
        }
        for i in node.interfaces.iter() {
            if i.name.text != iface_name {
                continue;
            }
            for m in &i.methods {
                if m.is_static {
                    continue;
                }
                let mode = registry_lookup(
                    &self.receiver_modes,
                    &format!("{iface_name}::{}", m.name.text),
                );
                out.push((m.name.text.clone(), mode));
            }
            return;
        }
    }
}
