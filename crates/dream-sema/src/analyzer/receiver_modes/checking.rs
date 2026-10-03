use super::*;

impl<'a> Analyzer<'a> {
    pub(super) fn check_receiver_contracts(
        &mut self,
        diagnostics: &mut DiagnosticBag,
        registry: &IndexMap<MethodKey, Entry>,
        resolved_edges: &IndexMap<MethodKey, Vec<(MethodKey, TextSpan)>>,
    ) {
        self.receiver_modes.clear();
        for (&key, entry) in registry {
            if entry.explicit == Some(ReceiverMode::Borrow)
                && (entry.direct_unique
                    || resolved_edges[&key]
                        .iter()
                        .any(|(target, _)| registry[target].is_unique()))
            {
                diagnostics.file_path = file_path_string(&entry.file);
                let owner = dream_types::display_name(
                    &self.type_ctx.interner,
                    &self.type_ctx.defs,
                    entry.owner,
                );
                diagnostics.report_error(
                    format!(
                        "method '{}.{}' is declared 'borrow' but its body mutates the instance — a 'borrow' method promises not to mutate '{}'. Change the declaration to 'unique fun', or move the mutation out",
                        owner, entry.name, owner
                    ),
                    Some(entry.first_mutate_span.unwrap_or(entry.decl_span)),
                );
            }
            self.receiver_modes.insert(key, entry.effective_mode());
        }

        for (&owner, interfaces) in &self.implements {
            for iface in interfaces {
                let Some(methods) = self.interface_methods.get(iface) else {
                    continue;
                };
                for (slot, method) in methods.iter().enumerate() {
                    let required = self
                        .receiver_modes
                        .get(&(*iface, slot))
                        .copied()
                        .unwrap_or(ReceiverMode::Borrow);
                    for key in self.receiver_method_keys(owner, &method.name.text) {
                        let Some(entry) = registry.get(&key) else {
                            continue;
                        };
                        let actual = entry.effective_mode();
                        if actual == required {
                            continue;
                        }
                        diagnostics.file_path = file_path_string(&entry.file);
                        let owner_name = dream_types::display_name(
                            &self.type_ctx.interner,
                            &self.type_ctx.defs,
                            owner,
                        );
                        let iface_name = dream_types::display_name(
                            &self.type_ctx.interner,
                            &self.type_ctx.defs,
                            *iface,
                        );
                        diagnostics.report_error(
                            format!(
                                "'{}.{}' is inferred {} but interface '{}' declares it {} — a caller holding the interface value must be able to rely on the same mutation rights. Align the two (mark the interface 'unique' if implementing classes mutate '{}', or make this method read-only)",
                                owner_name, entry.name,
                                match actual { ReceiverMode::Unique => "unique", ReceiverMode::Borrow => "borrow" },
                                iface_name,
                                match required { ReceiverMode::Unique => "'unique'", ReceiverMode::Borrow => "'borrow'" },
                                owner_name,
                            ),
                            Some(entry.decl_span),
                        );
                    }
                }
            }
        }
    }
}
