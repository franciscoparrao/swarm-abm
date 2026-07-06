//! `#[derive(MultiAgent)]`: heterogeneous agents without an `enum` full of
//! dead fields.
//!
//! Without this macro, a model with several agent types (e.g. SIGRID's 7
//! species) is forced into a single `struct` with a `Species` enum and
//! fields that only make sense for some species — the #1 adoption friction
//! that port exposed (see `docs/AUDIT.md`, P1-3).
//!
//! With the macro, each agent type is a normal `struct` with its own `impl
//! Agent`, and the enum that groups them only needs:
//!
//! ```ignore
//! #[derive(MultiAgent)]
//! enum Critter {
//!     Fox(Fox),
//!     Dog(Dog),
//! }
//! ```
//!
//! The macro generates `impl Agent for Critter`, dispatching
//! `decide`/`apply`/`step`/`stage` to the active variant's inner type. No
//! trait objects (`Box<dyn Agent>`): the dispatch is a static `match`, so
//! `AgentSet<Critter>`'s layout and the engine's determinism are unchanged.
//!
//! Requirements for each variant:
//! - Exactly one unnamed field: `Variant(Type)`.
//! - The inner `Type` implements [`Agent`](https://docs.rs/swarm-abm/latest/swarm_abm/agent/trait.Agent.html).
//! - All variants share the same `Agent::Model` (the first variant's is
//!   taken as the enum's `Model`; if some variant has a different `Model`,
//!   the generated `match` fails to compile — the error points at the
//!   conflicting type).
//!
//! ## Limitation: `decide_with_peers` is not dispatched
//!
//! [`Agent::decide_with_peers`](https://docs.rs/swarm-abm/latest/swarm_abm/agent/trait.Agent.html#method.decide_with_peers)
//! takes `peers: &AgentSet<Self>`. For a variant's inner type (say `Fox`),
//! that is `&AgentSet<Fox>`; for the enum itself it would be
//! `&AgentSet<Critter>` — a different type. There is no mechanical way to
//! forward the enum's peers snapshot into the inner type's method, so the
//! macro deliberately leaves `decide_with_peers` **un-dispatched**: the
//! enum inherits the trait's no-op default, same as if it were never
//! implemented. This is silent (no compile error), so if a model needs
//! peer-visibility with heterogeneous agents, do not rely on this derive —
//! hand-write `impl Agent for Critter` instead (matching on `self` and
//! composing the logic yourself, with whatever peers representation makes
//! sense for the model).

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Type, parse_macro_input};

#[proc_macro_derive(MultiAgent)]
pub fn derive_multi_agent(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let data_enum = match &input.data {
        Data::Enum(e) => e,
        _ => {
            return syn::Error::new_spanned(
                &input,
                "MultiAgent can only be derived on an enum (one variant per agent type)",
            )
            .to_compile_error()
            .into();
        }
    };

    if data_enum.variants.is_empty() {
        return syn::Error::new_spanned(&input, "MultiAgent requires at least one variant")
            .to_compile_error()
            .into();
    }

    let mut inner_types: Vec<Type> = Vec::new();
    let mut decide_arms = Vec::new();
    let mut apply_arms = Vec::new();
    let mut step_arms = Vec::new();
    let mut stage_arms = Vec::new();

    for variant in &data_enum.variants {
        let Fields::Unnamed(fields) = &variant.fields else {
            return syn::Error::new_spanned(
                variant,
                "each MultiAgent variant must wrap exactly one type, e.g. \
                 `Fox(Fox)` — unit variants and named fields aren't supported",
            )
            .to_compile_error()
            .into();
        };
        if fields.unnamed.len() != 1 {
            return syn::Error::new_spanned(
                variant,
                "each MultiAgent variant must wrap exactly one type \
                 (found more than one field)",
            )
            .to_compile_error()
            .into();
        }
        let ty = fields.unnamed.first().unwrap().ty.clone();
        let vident = &variant.ident;

        decide_arms.push(quote! {
            #name::#vident(inner) => ::swarm_abm::agent::Agent::decide(inner, id, model, rng),
        });
        apply_arms.push(quote! {
            #name::#vident(inner) => ::swarm_abm::agent::Agent::apply(inner, id, model, rng),
        });
        step_arms.push(quote! {
            #name::#vident(inner) => ::swarm_abm::agent::Agent::step(inner, id, model, rng),
        });
        stage_arms.push(quote! {
            #name::#vident(inner) => ::swarm_abm::agent::Agent::stage(inner, stage, id, model, rng),
        });

        inner_types.push(ty);
    }

    // The enum's Model is the first variant's; if some variant implements
    // Agent for a different Model, the `match` below fails to compile (each
    // arm requires its own type's Model) — the compiler's own type error
    // already points at the conflicting arm, so no redundant validation is
    // needed here.
    let model_ty = &inner_types[0];

    let expanded = quote! {
        impl ::swarm_abm::agent::Agent for #name {
            type Model = <#model_ty as ::swarm_abm::agent::Agent>::Model;

            fn decide(
                &mut self,
                id: ::swarm_abm::agent::AgentId,
                model: &Self::Model,
                rng: &mut ::swarm_abm::rng::SimRng,
            ) {
                match self {
                    #(#decide_arms)*
                }
            }

            fn apply(
                &mut self,
                id: ::swarm_abm::agent::AgentId,
                model: &mut Self::Model,
                rng: &mut ::swarm_abm::rng::SimRng,
            ) {
                match self {
                    #(#apply_arms)*
                }
            }

            fn step(
                &mut self,
                id: ::swarm_abm::agent::AgentId,
                model: &mut Self::Model,
                rng: &mut ::swarm_abm::rng::SimRng,
            ) {
                match self {
                    #(#step_arms)*
                }
            }

            fn stage(
                &mut self,
                stage: usize,
                id: ::swarm_abm::agent::AgentId,
                model: &mut Self::Model,
                rng: &mut ::swarm_abm::rng::SimRng,
            ) {
                match self {
                    #(#stage_arms)*
                }
            }
        }
    };

    expanded.into()
}
