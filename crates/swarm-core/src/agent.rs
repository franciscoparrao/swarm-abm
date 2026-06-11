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
/// Mientras un agente ejecuta cualquiera de sus fases está temporalmente
/// *fuera* del [`AgentSet`] del modelo (patrón take-out), de modo que puede
/// acceder al modelo —incluidos otros agentes— sin conflicto de préstamos.
///
/// Hay dos estilos de implementación:
///
/// - **Secuencial** ([`Activation::Ordered`](crate::schedule::Activation) /
///   `Random`): implementa solo [`step`](Self::step), que recibe el modelo
///   mutable.
/// - **Simultáneo** ([`Activation::Simultaneous`](crate::schedule::Activation)):
///   implementa [`decide`](Self::decide) y [`apply`](Self::apply). En la fase
///   de decisión **todos** los agentes observan el mismo estado del mundo: el
///   modelo llega *inmutable* (`&Model`), así que el compilador garantiza que
///   nadie escribe estado compartido antes del commit. La decisión se guarda
///   en campos propios del agente y se materializa en `apply`.
///
/// Un modelo escrito en estilo `decide`/`apply` también funciona bajo
/// activación secuencial: el `step` por defecto ejecuta `decide` + `apply`
/// de inmediato.
pub trait Agent: Sized {
    /// Modelo al que pertenece este agente.
    type Model;

    /// Fase de decisión (activación simultánea): observa el modelo y registra
    /// la decisión en `self`. No hace nada por defecto.
    ///
    /// Convención: los campos de decisión de otros agentes pueden ya estar
    /// escritos en esta fase; lee solo su estado "actual", no sus decisiones.
    fn decide(&mut self, _id: AgentId, _model: &Self::Model, _rng: &mut SimRng) {}

    /// Fase de aplicación (activación simultánea): materializa en el modelo
    /// lo decidido en [`decide`](Self::decide). No hace nada por defecto.
    ///
    /// Se ejecuta en orden de inserción; si dos decisiones colisionan (p. ej.
    /// dos agentes eligieron la misma celda), resolver aquí re-verificando.
    fn apply(&mut self, _id: AgentId, _model: &mut Self::Model, _rng: &mut SimRng) {}

    /// Un paso de comportamiento bajo activación secuencial. Por defecto
    /// ejecuta [`decide`](Self::decide) seguido de [`apply`](Self::apply).
    fn step(&mut self, id: AgentId, model: &mut Self::Model, rng: &mut SimRng) {
        self.decide(id, model, rng);
        self.apply(id, model, rng);
    }
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
        let mut out = Vec::with_capacity(self.live);
        self.collect_ids_into(&mut out);
        out
    }

    /// Vuelca los ids vivos en `out` (lo limpia primero), reutilizando su
    /// capacidad. Evita asignar un `Vec` nuevo por paso en hot loops.
    pub fn collect_ids_into(&self, out: &mut Vec<AgentId>) {
        out.clear();
        out.extend(
            self.slots
                .iter()
                .enumerate()
                .filter_map(|(i, s)| s.as_ref().map(|_| AgentId(i))),
        );
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
