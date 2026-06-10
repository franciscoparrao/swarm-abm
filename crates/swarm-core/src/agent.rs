//! Trait [`Agent`] y almacenamiento de agentes ([`AgentSet`]).

use crate::rng::SimRng;

/// Identificador estable de un agente dentro de un [`AgentSet`].
///
/// En v0.1 los slots no se reutilizan tras un `remove`, por lo que un
/// `AgentId` nunca pasa a referirse a otro agente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentId(usize);

impl AgentId {
    /// Índice interno del agente (útil para depuración o indexar arrays propios).
    #[must_use]
    pub fn as_usize(self) -> usize {
        self.0
    }
}

/// Comportamiento de un agente.
///
/// El agente recibe su propio `id`, acceso mutable al modelo completo y el
/// RNG de la simulación. Mientras un agente ejecuta su `step`, está
/// temporalmente *fuera* del [`AgentSet`] del modelo (patrón take-out), de
/// modo que puede mutar el modelo —incluidos otros agentes— sin conflicto
/// de préstamos.
pub trait Agent: Sized {
    /// Modelo al que pertenece este agente.
    type Model;

    /// Un paso de comportamiento del agente.
    fn step(&mut self, id: AgentId, model: &mut Self::Model, rng: &mut SimRng);
}

/// Colección de agentes con identificadores estables.
///
/// Inserciones devuelven un [`AgentId`]; las eliminaciones dejan el slot
/// vacío (no se reutiliza en v0.1). La iteración sigue el orden de inserción.
///
/// **Limitación v0.1**: un agente no puede eliminarse a sí mismo durante su
/// propio `step` (está fuera del set en ese momento). Registra la baja en el
/// estado del modelo y elimínalo en [`Model::after_step`](crate::model::Model::after_step).
#[derive(Debug, Default)]
pub struct AgentSet<A> {
    slots: Vec<Option<A>>,
    live: usize,
}

impl<A> AgentSet<A> {
    /// Crea un set vacío.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            live: 0,
        }
    }

    /// Crea un set vacío con capacidad pre-reservada.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            live: 0,
        }
    }

    /// Inserta un agente y devuelve su identificador.
    pub fn insert(&mut self, agent: A) -> AgentId {
        let id = AgentId(self.slots.len());
        self.slots.push(Some(agent));
        self.live += 1;
        id
    }

    /// Referencia al agente, si sigue vivo (y no está siendo procesado).
    #[must_use]
    pub fn get(&self, id: AgentId) -> Option<&A> {
        self.slots.get(id.0).and_then(Option::as_ref)
    }

    /// Referencia mutable al agente, si sigue vivo.
    #[must_use]
    pub fn get_mut(&mut self, id: AgentId) -> Option<&mut A> {
        self.slots.get_mut(id.0).and_then(Option::as_mut)
    }

    /// Elimina un agente del set y lo devuelve.
    pub fn remove(&mut self, id: AgentId) -> Option<A> {
        let agent = self.slots.get_mut(id.0).and_then(Option::take);
        if agent.is_some() {
            self.live -= 1;
        }
        agent
    }

    /// Número de agentes vivos.
    #[must_use]
    pub fn len(&self) -> usize {
        self.live
    }

    /// `true` si no hay agentes vivos.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Identificadores de todos los agentes vivos, en orden de inserción.
    #[must_use]
    pub fn ids(&self) -> Vec<AgentId> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|_| AgentId(i)))
            .collect()
    }

    /// Itera sobre `(id, &agente)` en orden de inserción.
    pub fn iter(&self) -> impl Iterator<Item = (AgentId, &A)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|a| (AgentId(i), a)))
    }

    /// Itera sobre `(id, &mut agente)` en orden de inserción.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (AgentId, &mut A)> {
        self.slots
            .iter_mut()
            .enumerate()
            .filter_map(|(i, s)| s.as_mut().map(|a| (AgentId(i), a)))
    }

    /// Saca temporalmente un agente para ejecutar su `step` (patrón take-out).
    pub(crate) fn take(&mut self, id: AgentId) -> Option<A> {
        self.slots.get_mut(id.0).and_then(Option::take)
    }

    /// Devuelve al set un agente sacado con [`take`](Self::take).
    pub(crate) fn put_back(&mut self, id: AgentId, agent: A) {
        if let Some(slot) = self.slots.get_mut(id.0) {
            *slot = Some(agent);
        }
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
        // El slot no se reutiliza: el id de b sigue siendo válido.
        assert_eq!(set.get(b), Some(&"b"));
        // Doble remove es inocuo.
        assert_eq!(set.remove(a), None);
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn ids_e_iter_en_orden_de_insercion() {
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
}
