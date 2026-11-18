use super::{Decl, Linkage, ModuleWriter, fmt, write_declaration, write_global};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

pub(super) struct Definition {
    pub(super) group: usize,
    pub(super) name: String,
    pub(super) linkage: Linkage,
    pub(super) declaration: Decl,
    pub(super) range: Range<usize>,
}

#[derive(Default)]
pub(super) struct Units {
    pub(super) current: usize,
    pub(super) definitions: Vec<Definition>,
}

impl ModuleWriter {
    pub fn enable_units(&mut self) {
        self.units = Some(Units::default());
    }

    pub fn set_unit(&mut self, unit: usize) {
        if let Some(units) = &mut self.units {
            units.current = unit;
        }
    }
}

impl Units {
    pub(super) fn render(&self, module: &ModuleWriter, header: &str) -> Vec<String> {
        let mut groups = BTreeMap::<usize, Vec<&Definition>>::new();
        for definition in &self.definitions {
            groups.entry(definition.group).or_default().push(definition);
        }
        if groups.len() < 2 {
            return Vec::new();
        }

        let mut refs = BTreeMap::new();
        let mut last_group = BTreeMap::<&str, usize>::new();
        let mut shared = BTreeSet::new();
        for (&group, definitions) in &groups {
            let mut names = BTreeSet::new();
            for definition in definitions {
                names.extend(global_references(&module.bodies[definition.range.clone()]));
            }
            if group == 0 {
                for g in module.globals.values() {
                    if let Some(init) = &g.init {
                        names.extend(global_references(init));
                    }
                }
            }
            for &name in &names {
                if let Some(previous) = last_group.insert(name, group)
                    && previous != group
                {
                    shared.insert(name);
                }
            }
            refs.insert(group, names);
        }

        let mut metadata = String::new();
        module.md.write(&mut metadata);
        groups
            .into_iter()
            .map(|(group, definitions)| {
                let names = &refs[&group];
                let mut out = header.to_owned();
                for (name, global) in &module.globals {
                    let ident = fmt::ident(name);
                    if group == 0 {
                        let escapes = last_group
                            .get(ident.as_str())
                            .is_some_and(|&owner| owner != 0)
                            || shared.contains(ident.as_str());
                        write_global(
                            &mut out,
                            name,
                            global,
                            visible(global.linkage, escapes),
                            global.init.as_deref(),
                        );
                    } else if names.contains(ident.as_str()) {
                        write_global(&mut out, name, global, global.linkage, None);
                    }
                }
                for (name, declaration) in &module.decls {
                    if !module.defined.contains(name) && names.contains(fmt::ident(name).as_str()) {
                        write_declaration(&mut out, name, declaration);
                    }
                }
                for definition in &self.definitions {
                    if group != definition.group
                        && names.contains(fmt::ident(&definition.name).as_str())
                    {
                        write_declaration(&mut out, &definition.name, &definition.declaration);
                    }
                }
                out.push('\n');
                for definition in definitions {
                    let body = &module.bodies[definition.range.clone()];
                    // The symbol on the define line is included in its owner's reference set.
                    if visible(
                        definition.linkage,
                        shared.contains(fmt::ident(&definition.name).as_str()),
                    ) == Linkage::Hidden
                    {
                        let prefix = definition.linkage.prefix();
                        out.push_str("define hidden ");
                        out.push_str(&body["define ".len() + prefix.len()..]);
                    } else {
                        out.push_str(body);
                    }
                }
                out.push_str(&metadata);
                out
            })
            .collect()
    }
}

fn visible(linkage: Linkage, escapes: bool) -> Linkage {
    if escapes && matches!(linkage, Linkage::Internal | Linkage::Private) {
        Linkage::Hidden
    } else {
        linkage
    }
}

// Names use the same lexical spelling as the typed writer's @ operands. Extra matches in string
// constants can retain a declaration, but cannot suppress one used by an instruction.
fn global_references(text: &str) -> impl Iterator<Item = &str> {
    text.match_indices('@').filter_map(|(index, _)| {
        let rest = &text[index + 1..];
        if let Some(quoted) = rest.strip_prefix('"') {
            return Some(&rest[..quoted.find('"')? + 2]);
        }
        let length = rest
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$' | b'-'))
            .count();
        (length > 0).then_some(&rest[..length])
    })
}
