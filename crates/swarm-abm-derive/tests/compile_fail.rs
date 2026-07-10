//! UI tests: `#[derive(MultiAgent)]` on invalid inputs must produce a
//! compile error whose message points at the problem, not a macro panic nor
//! an inscrutable generic type error. The exact `.stderr` outputs ARE pinned
//! under `tests/ui/`: trybuild requires them for `compile_fail` tests (with
//! no `.stderr` it writes candidates to `wip/` and fails the test). If a new
//! rustc changes the diagnostic rendering, refresh them with
//! `TRYBUILD=overwrite cargo test -p swarm-abm-derive` and review the diff.

#[test]
fn ui_rechaza_entradas_invalidas() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
