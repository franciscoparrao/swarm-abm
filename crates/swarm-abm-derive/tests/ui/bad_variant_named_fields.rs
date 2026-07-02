use swarm_abm_derive::MultiAgent;

#[derive(MultiAgent)]
enum Bad {
    Foo { x: i32 },
}

fn main() {}
