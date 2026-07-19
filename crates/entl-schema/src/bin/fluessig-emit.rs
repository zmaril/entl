//! Print entl's `catalog.json` (the derive front end's replacement for
//! `node emit.mjs entl.tsp`). `scripts/gen.sh` writes stdout to
//! `schema/catalog.json`, then hands it to `fluessig-gen`.

fn main() {
    print!("{}", entl_schema::fluessig_catalog::to_json());
}
