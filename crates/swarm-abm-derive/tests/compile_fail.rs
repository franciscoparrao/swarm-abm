//! Tests de UI: `#[derive(MultiAgent)]` sobre entradas inválidas debe dar un
//! error de compilación con un mensaje que señale el problema, no un panic
//! del macro ni un error genérico de tipos incomprensible. No se fijan los
//! `.stderr` exactos (frágil entre versiones de rustc): alcanza con que
//! `trybuild` confirme que la compilación falla.

#[test]
fn ui_rechaza_entradas_invalidas() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
