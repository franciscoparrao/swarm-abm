//! [`Agent`] trait and agent storage ([`AgentSet`]).

use crate::rng::{SimRng, child_rng};

/// Stable identifier of an agent within an [`AgentSet`].
///
/// It is a **generational arena** handle (`slotmap`-style): a slot index
/// plus a generation. When an agent is removed, its slot can be reused for a
/// **future**, distinct agent — but the generation stored in the slot is
/// incremented on removal, so an `AgentId` issued before the removal
/// **never** resolves to an agent again (neither to the one it originally
/// represented, which no longer exists, nor to the new one occupying its
/// slot): [`get`](AgentSet::get)/[`remove`](AgentSet::remove) compare the
/// generation and return `None` on mismatch. This is the classic ABA
/// problem, which a generational arena avoids by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AgentId {
    index: u32,
    generation: u32,
}

impl AgentId {
    /// Internal slot index (useful for indexing your own array *while* the
    /// agent is still alive).
    ///
    /// **Not a long-term stable key**: after a [`remove`](AgentSet::remove),
    /// the engine may reuse this same index for a different agent on a
    /// later `insert` — two `AgentId`s with the same `as_usize()` but
    /// different generation refer to different agents. If your model has
    /// removals, use the full `AgentId` (`Eq`/`Hash`) as the key, not just
    /// this index.
    #[must_use]
    pub fn as_usize(self) -> usize {
        self.index as usize
    }
}

/// Behavior of an agent.
///
/// While an agent is running any of its phases, it is temporarily *out of*
/// the model's [`AgentSet`] (take-out pattern), so it can access the model
/// — including other agents — without a borrow conflict.
///
/// There are two implementation styles:
///
/// - **Sequential** ([`Activation::Ordered`](crate::schedule::Activation) /
///   `Random`): implement only [`step`](Self::step), which receives the
///   mutable model.
/// - **Simultaneous** ([`Activation::Simultaneous`](crate::schedule::Activation)):
///   implement [`decide`](Self::decide) and [`apply`](Self::apply). During
///   the decision phase **every** agent observes the same world state: the
///   model arrives *immutable* (`&Model`), so the compiler guarantees that
///   no one writes shared state before the commit. The decision is stored
///   in the agent's own fields and materialized in `apply`.
///
/// A model written in `decide`/`apply` style also works under sequential
/// activation: the default `step` runs `decide` + `apply` immediately.
///
/// ## Limitation of `decide`: it cannot see other agents
///
/// During the `decide` phase, the model's `AgentSet` is empty (double-buffer
/// pattern: see [`Simulation::step`](crate::sim::Simulation::step)) — so
/// `decide` can only observe the *environment* (grid, space, graph), never
/// the state of other agents. The archetypal case of simultaneous
/// activation, "everyone decides looking at the previous state of
/// everyone", then requires the user to manually duplicate into the
/// environment any agent data that others need to read (e.g. positions in
/// an auxiliary grid). To avoid that manual plumbing, use
/// [`decide_with_peers`](Self::decide_with_peers): an **additive** method
/// (it does not replace `decide`) that additionally receives a frozen
/// snapshot of the entire `AgentSet` as it was at the start of the phase,
/// invoked by
/// [`Simulation::step_with_peers`](crate::sim::Simulation::step_with_peers).
pub trait Agent: Sized {
    /// Model this agent belongs to.
    type Model;

    /// Decision phase (simultaneous activation): observes the model and
    /// records the decision in `self`. Does nothing by default.
    ///
    /// Convention: other agents' decision fields may already be written
    /// during this phase; read only their "current" state, not their
    /// decisions.
    fn decide(&mut self, _id: AgentId, _model: &Self::Model, _rng: &mut SimRng) {}

    /// Like [`decide`](Self::decide), but additionally receives `peers`: a
    /// frozen snapshot of **the entire** `AgentSet` (including `self`) as it
    /// was at the start of the phase — before any agent had decided. Allows
    /// a decision to depend on the state of other agents without manually
    /// duplicating that state into the environment. Does nothing by default
    /// (identical behavior to not implementing it).
    ///
    /// Only invoked via
    /// [`step_with_peers`](crate::sim::Simulation::step_with_peers) /
    /// [`run_with_peers`](crate::sim::Simulation::run_with_peers) — normal
    /// `step`/`run` never call it, so a model that doesn't need it pays no
    /// cost at all (not even the requirement that `Self: Clone`, which
    /// `step_with_peers` does require in order to take the snapshot).
    fn decide_with_peers(
        &mut self,
        _id: AgentId,
        _model: &Self::Model,
        _peers: &AgentSet<Self>,
        _rng: &mut SimRng,
    ) {
    }

    /// Application phase (simultaneous activation): materializes into the
    /// model what was decided in [`decide`](Self::decide). Does nothing by
    /// default.
    ///
    /// Runs in slot-index order — which equals insertion order until a
    /// removal is followed by an insertion (see [`AgentSet`]); if two
    /// decisions collide (e.g. two agents chose the same cell), resolve it
    /// here by re-checking.
    fn apply(&mut self, _id: AgentId, _model: &mut Self::Model, _rng: &mut SimRng) {}

    /// A single step of behavior under sequential activation. Runs
    /// [`decide`](Self::decide) followed by [`apply`](Self::apply) by
    /// default.
    fn step(&mut self, id: AgentId, model: &mut Self::Model, rng: &mut SimRng) {
        self.decide(id, model, rng);
        self.apply(id, model, rng);
    }

    /// Behavior in stage `stage` of a
    /// [`Activation::Staged`](crate::schedule::Activation::Staged)
    /// activation: unlike [`decide`](Self::decide) (immutable model, meant
    /// for parallelization), each stage receives the **mutable** model — all
    /// stages are symmetric, like the named phases of Mesa's
    /// `StagedActivation` ("move", "eat", "reproduce", ...). The engine
    /// guarantees that **all** agents complete stage `s` before any of them
    /// enters stage `s+1`. Does nothing by default.
    fn stage(&mut self, _stage: usize, _id: AgentId, _model: &mut Self::Model, _rng: &mut SimRng) {}
}

/// State of an arena slot.
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum SlotState<A> {
    /// Agent alive and accessible.
    Occupied(A),
    /// Temporarily taken out to run its `step` (take-out pattern): the
    /// value is not here, it is borrowed by whoever called
    /// [`AgentSet::take`]. `pending_removal` lets the agent itself remove
    /// itself by calling `remove(its_id)` **during** its step: since the
    /// slot cannot be freed on the spot (the value is out, borrowed), it is
    /// only flagged here; [`AgentSet::put_back`] actually frees the slot and
    /// discards the value the caller was trying to return.
    TakenOut { pending_removal: bool },
    /// Vacant: part of the (LIFO) free list of reusable slots.
    Free { next: Option<u32> },
}

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct Slot<A> {
    generation: u32,
    state: SlotState<A>,
}

/// Collection of agents with stable identifiers: a generational arena (see
/// [`AgentId`]).
///
/// Insertions return an [`AgentId`]; removals free the slot for a future
/// `insert` (unlike a `Vec` with permanent holes, memory is bounded by the
/// **peak** of live population, not by the total historical count of
/// additions). Iteration follows **slot index** order, which matches
/// insertion order only as long as there have been no removals followed by
/// insertions (a reused slot inherits the position of the one that freed
/// its index, not that of the agent now occupying it).
///
/// `Clone` (when `A: Clone`) is what allows taking the "snapshot" for
/// [`decide_with_peers`](Agent::decide_with_peers): a `Vec` of slots with a
/// generation is trivially clonable, requiring no special logic.
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AgentSet<A> {
    slots: Vec<Slot<A>>,
    /// Head of the free list of vacant slots, or `None` if there are none
    /// (the next `insert` must append a new slot).
    free_head: Option<u32>,
    live: usize,
}

impl<A> std::fmt::Debug for AgentSet<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentSet")
            .field("live", &self.live)
            .field("slots", &self.slots.len())
            .finish()
    }
}

// Manual (not derived) Default, so we don't require `A: Default`.
impl<A> Default for AgentSet<A> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free_head: None,
            live: 0,
        }
    }
}

impl<A> AgentSet<A> {
    /// Creates an empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty set with pre-reserved capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            free_head: None,
            live: 0,
        }
    }

    /// Inserts an agent and returns its identifier. Reuses the most
    /// recently freed vacant slot if one exists (LIFO free list, good cache
    /// locality); otherwise appends a new slot.
    pub fn insert(&mut self, agent: A) -> AgentId {
        if let Some(index) = self.free_head {
            let slot = &mut self.slots[index as usize];
            let SlotState::Free { next } = slot.state else {
                unreachable!("free_head always points to a Free slot")
            };
            self.free_head = next;
            slot.state = SlotState::Occupied(agent);
            self.live += 1;
            return AgentId {
                index,
                generation: slot.generation,
            };
        }
        // The limit is on *slots* (peak concurrent population), not on
        // historical insertions: the LIFO free list recycles indices, so
        // `slots.len()` only grows when the live count sets a new peak.
        let index =
            u32::try_from(self.slots.len()).expect("more than u32::MAX concurrent agent slots");
        self.slots.push(Slot {
            generation: 0,
            state: SlotState::Occupied(agent),
        });
        self.live += 1;
        AgentId {
            index,
            generation: 0,
        }
    }

    /// Reference to the agent, if it is still alive (and not currently
    /// being processed).
    #[must_use]
    pub fn get(&self, id: AgentId) -> Option<&A> {
        let slot = self.slots.get(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        match &slot.state {
            SlotState::Occupied(a) => Some(a),
            SlotState::TakenOut { .. } | SlotState::Free { .. } => None,
        }
    }

    /// Mutable reference to the agent, if it is still alive.
    #[must_use]
    pub fn get_mut(&mut self, id: AgentId) -> Option<&mut A> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        match &mut slot.state {
            SlotState::Occupied(a) => Some(a),
            SlotState::TakenOut { .. } | SlotState::Free { .. } => None,
        }
    }

    /// Removes an agent from the set and returns it, freeing its slot for a
    /// future `insert`.
    ///
    /// If `id` refers to the agent that is currently running its own
    /// `step`/`apply` (self-remove), the slot cannot be freed yet — its
    /// value is borrowed by the caller of
    /// [`step`](Agent::step)/[`apply`](Agent::apply) — so it is only
    /// flagged for release and this call returns `None`; the slot is
    /// actually freed when the engine calls `put_back` (crate-internal) at
    /// the end of that phase.
    pub fn remove(&mut self, id: AgentId) -> Option<A> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        match &mut slot.state {
            SlotState::Occupied(_) => {
                let SlotState::Occupied(value) = std::mem::replace(
                    &mut slot.state,
                    SlotState::Free {
                        next: self.free_head,
                    },
                ) else {
                    unreachable!()
                };
                slot.generation = slot.generation.wrapping_add(1);
                self.free_head = Some(id.index);
                self.live -= 1;
                Some(value)
            }
            SlotState::TakenOut {
                pending_removal: pending @ false,
            } => {
                *pending = true;
                self.live -= 1;
                None
            }
            SlotState::TakenOut { .. } | SlotState::Free { .. } => None,
        }
    }

    /// Number of live agents.
    #[must_use]
    pub fn len(&self) -> usize {
        self.live
    }

    /// `true` if there are no live agents.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Identifiers of all live agents, in slot-index order (see the struct
    /// note about slot reuse).
    #[must_use]
    pub fn ids(&self) -> Vec<AgentId> {
        let mut out = Vec::with_capacity(self.live);
        self.collect_ids_into(&mut out);
        out
    }

    /// Dumps the live ids into `out` (clearing it first), reusing its
    /// capacity. Avoids allocating a new `Vec` per step in hot loops.
    pub fn collect_ids_into(&self, out: &mut Vec<AgentId>) {
        out.clear();
        out.extend(self.slots.iter().enumerate().filter_map(|(i, slot)| {
            matches!(slot.state, SlotState::Occupied(_)).then_some(AgentId {
                index: i as u32,
                generation: slot.generation,
            })
        }));
    }

    /// Iterates over `(id, &agent)` in slot-index order.
    pub fn iter(&self) -> impl Iterator<Item = (AgentId, &A)> {
        self.slots.iter().enumerate().filter_map(|(i, slot)| {
            let SlotState::Occupied(a) = &slot.state else {
                return None;
            };
            Some((
                AgentId {
                    index: i as u32,
                    generation: slot.generation,
                },
                a,
            ))
        })
    }

    /// Iterates over `(id, &mut agent)` in slot-index order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (AgentId, &mut A)> {
        self.slots.iter_mut().enumerate().filter_map(|(i, slot)| {
            let generation = slot.generation;
            let SlotState::Occupied(a) = &mut slot.state else {
                return None;
            };
            Some((
                AgentId {
                    index: i as u32,
                    generation,
                },
                a,
            ))
        })
    }

    /// Temporarily takes an agent out to run its `step` (take-out pattern).
    pub(crate) fn take(&mut self, id: AgentId) -> Option<A> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        if !matches!(slot.state, SlotState::Occupied(_)) {
            return None;
        }
        let SlotState::Occupied(value) = std::mem::replace(
            &mut slot.state,
            SlotState::TakenOut {
                pending_removal: false,
            },
        ) else {
            unreachable!()
        };
        Some(value)
    }

    /// Returns to the set an agent taken out with [`take`](Self::take). If
    /// the agent self-removed during its step (see
    /// [`remove`](Self::remove)), frees the slot instead of restoring it and
    /// discards `agent`.
    pub(crate) fn put_back(&mut self, id: AgentId, agent: A) {
        let Some(slot) = self.slots.get_mut(id.index as usize) else {
            return;
        };
        if slot.generation != id.generation {
            return;
        }
        match slot.state {
            SlotState::TakenOut {
                pending_removal: true,
            } => {
                slot.state = SlotState::Free {
                    next: self.free_head,
                };
                slot.generation = slot.generation.wrapping_add(1);
                self.free_head = Some(id.index);
                // `agent` is dropped: the caller can no longer "undo" the
                // remove requested during the step.
            }
            SlotState::TakenOut {
                pending_removal: false,
            } => {
                slot.state = SlotState::Occupied(agent);
            }
            SlotState::Occupied(_) | SlotState::Free { .. } => {
                // Should not happen if take/put_back are called in pairs.
            }
        }
    }
}

impl<A: Agent> AgentSet<A> {
    /// `decide` phase (simultaneous activation), **sequential**: each agent
    /// decides with its per-agent RNG — deterministic by `(seed, step, id)`
    /// — observing the immutable model. The set must be separate from the
    /// model (double-buffer pattern): `decide` reads the environment and a
    /// snapshot, not the live set.
    pub(crate) fn decide_all(&mut self, model: &A::Model, seed: u64, step: u64) {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if let SlotState::Occupied(agent) = &mut slot.state {
                let mut rng = child_rng(seed, step, i as u64);
                let id = AgentId {
                    index: i as u32,
                    generation: slot.generation,
                };
                agent.decide(id, model, &mut rng);
            }
        }
    }

    /// Same as [`decide_all`](Self::decide_all) but **in parallel** (rayon).
    ///
    /// Produces the **exact same bit-for-bit result** as the sequential
    /// version: each agent's RNG depends only on `(seed, step, id)` — never
    /// on the thread — and `decide` receives the *immutable* model, so the
    /// compiler guarantees that no agent writes shared state during the
    /// phase. It is this type-proven immutability that makes the
    /// parallelism safe.
    #[cfg(feature = "parallel")]
    pub(crate) fn decide_all_par(&mut self, model: &A::Model, seed: u64, step: u64)
    where
        A: Send,
        A::Model: Sync,
    {
        use rayon::prelude::*;
        self.slots.par_iter_mut().enumerate().for_each(|(i, slot)| {
            if let SlotState::Occupied(agent) = &mut slot.state {
                let mut rng = child_rng(seed, step, i as u64);
                let id = AgentId {
                    index: i as u32,
                    generation: slot.generation,
                };
                agent.decide(id, model, &mut rng);
            }
        });
    }

    /// Like [`decide_all`](Self::decide_all), but invokes
    /// [`Agent::decide_with_peers`] with `peers` (the frozen snapshot of the
    /// set at the start of the phase) instead of [`Agent::decide`].
    pub(crate) fn decide_all_with_peers(
        &mut self,
        model: &A::Model,
        peers: &AgentSet<A>,
        seed: u64,
        step: u64,
    ) {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if let SlotState::Occupied(agent) = &mut slot.state {
                let mut rng = child_rng(seed, step, i as u64);
                let id = AgentId {
                    index: i as u32,
                    generation: slot.generation,
                };
                agent.decide_with_peers(id, model, peers, &mut rng);
            }
        }
    }

    /// Parallel variant of
    /// [`decide_all_with_peers`](Self::decide_all_with_peers), with the same
    /// bit-for-bit identity guarantee as
    /// [`decide_all_par`](Self::decide_all_par): `peers` is an immutable
    /// snapshot shared across threads, and each agent's RNG depends only on
    /// `(seed, step, id)`.
    #[cfg(feature = "parallel")]
    pub(crate) fn decide_all_with_peers_par(
        &mut self,
        model: &A::Model,
        peers: &AgentSet<A>,
        seed: u64,
        step: u64,
    ) where
        // `Sync` in addition to `Send`: unlike `decide_all_par`, here
        // `peers` is shared by reference across all threads at once (not
        // just an exclusive value moved per thread).
        A: Send + Sync,
        A::Model: Sync,
    {
        use rayon::prelude::*;
        self.slots.par_iter_mut().enumerate().for_each(|(i, slot)| {
            if let SlotState::Occupied(agent) = &mut slot.state {
                let mut rng = child_rng(seed, step, i as u64);
                let id = AgentId {
                    index: i as u32,
                    generation: slot.generation,
                };
                agent.decide_with_peers(id, model, peers, &mut rng);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove() {
        let mut set = AgentSet::new();
        let a = set.insert("a");
        let b = set.insert("b");
        assert_eq!(set.len(), 2);
        assert_eq!(set.get(a), Some(&"a"));
        assert_eq!(set.remove(a), Some("a"));
        assert_eq!(set.get(a), None);
        assert_eq!(set.len(), 1);
        // b's id is still valid (it does not share an index with a).
        assert_eq!(set.get(b), Some(&"b"));
        // Double remove is harmless.
        assert_eq!(set.remove(a), None);
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn slot_reutilizado_invalida_el_id_viejo() {
        let mut set = AgentSet::new();
        let a = set.insert("a");
        assert_eq!(set.remove(a), Some("a"));
        // The next insert reuses a's slot (LIFO free list).
        let c = set.insert("c");
        assert_eq!(c.as_usize(), a.as_usize(), "reuses the same index");
        assert_ne!(c, a, "but the full AgentId differs (generation)");
        // The old id does NOT resolve to the new agent (avoids the ABA problem).
        assert_eq!(set.get(a), None);
        assert_eq!(set.get(c), Some(&"c"));
    }

    #[test]
    fn memoria_acotada_por_pico_no_por_historico() {
        let mut set = AgentSet::new();
        for i in 0..1000 {
            let id = set.insert(i);
            set.remove(id);
        }
        // 1000 alternating insertions and removals should never grow beyond
        // one slot: each insert reuses the single slot freed by the
        // previous remove.
        let id = set.insert(9999);
        assert_eq!(id.as_usize(), 0);
        let _ = id;
    }

    #[test]
    fn ids_e_iter_en_orden_de_indice() {
        let mut set = AgentSet::new();
        let ids: Vec<_> = (0..5).map(|i| set.insert(i)).collect();
        set.remove(ids[2]);
        assert_eq!(set.ids(), vec![ids[0], ids[1], ids[3], ids[4]]);
        let vivos: Vec<i32> = set.iter().map(|(_, &v)| v).collect();
        assert_eq!(vivos, vec![0, 1, 3, 4]);
    }

    #[test]
    fn take_y_put_back() {
        let mut set = AgentSet::new();
        let id = set.insert(10);
        let agente = set.take(id);
        assert_eq!(agente, Some(10));
        assert_eq!(set.get(id), None);
        set.put_back(id, 10);
        assert_eq!(set.get(id), Some(&10));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn self_remove_durante_take_out_libera_el_slot_en_put_back() {
        let mut set = AgentSet::new();
        let id = set.insert(10);
        let agente = set.take(id).expect("just inserted, must be alive");
        // The agent itself requests its removal while it is "out"
        // (simulates what `apply`/`step` would do by calling
        // `model.agents_mut().remove(id)` on its own id).
        assert_eq!(set.remove(id), None, "cannot return the value: it is out");
        assert_eq!(set.len(), 0, "but it already counts as removed");
        set.put_back(id, agente); // the engine tries to return it anyway
        assert_eq!(
            set.get(id),
            None,
            "put_back must free the slot, not restore it"
        );
        assert_eq!(set.len(), 0);
        // The slot is now free: a later insert reuses it.
        let nuevo = set.insert(20);
        assert_eq!(nuevo.as_usize(), id.as_usize());
    }
}
