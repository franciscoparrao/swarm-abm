//! Modelos ABM de referencia construidos sobre [`swarm_abm`].
//!
//! Cada modelo vive aquí —no como binario suelto— para que la **misma**
//! implementación la reutilicen los ejemplos (`examples/*`), los bindings
//! Python (`swarm-py`) y los benchmarks. Así la física del modelo no se
//! duplica entre el ejecutable y el binding (donde divergiría en silencio).
//!
//! Cada submódulo expone: un `…Config` con parámetros (`Default` razonable),
//! una función `build(config, seed)` que arma el modelo, y los métodos de
//! consulta que un recolector de datos necesita.

#![warn(missing_docs)]

pub mod schelling;
pub mod sir;
pub mod sugarscape;
