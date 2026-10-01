use super::*;

pub(super) fn apply_wraps(f: &mut MirFunction, sites: &[WrapSite]) {
    let mut sites = sites.to_vec();
    sites.sort_by_key(|s| (std::cmp::Reverse(s.birth_bi), std::cmp::Reverse(s.birth_si)));
    for site in sites {
        let mut death_si = site.death_si;
        f.blocks[site.birth_bi]
            .stmts
            .insert(site.birth_si, Statement::RegionEnter);
        if site.death_bi == site.birth_bi && death_si >= site.birth_si {
            death_si += 1;
        }
        let mut leave = vec![Statement::RegionLeave];
        for loc in &site.null_locals {
            leave.push(Statement::Assign(
                Place::Local(Local(*loc)),
                Rvalue::Use(Operand::Const(Const::Null)),
            ));
        }
        if site.replace_death {
            f.blocks[site.death_bi].stmts[death_si] = Statement::RegionLeave;
        } else {
            let block = &mut f.blocks[site.death_bi];
            let tail = block.stmts.split_off(death_si);
            block.stmts.extend(leave);
            block.stmts.extend(tail);
        }
    }
}
