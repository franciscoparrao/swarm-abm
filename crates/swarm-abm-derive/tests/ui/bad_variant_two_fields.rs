use swarm_abm_derive::MultiAgent;

#[derive(MultiAgent)]
enum Bad {
    Foo(i32, i32),
}

fn main() {}
