//! Modelo ABM de flujos de detritos (evento Atacama 2015) sobre swarm-abm.
//!
//! Port fiel de `debris-flow-abm` (Mesa/Python, V4 HYBRID v2). Es el primer
//! "modelo cliente" real del motor: valida que la API generaliza más allá de
//! los ejemplos canónicos (agentes heterogéneos vía enum, spawning dinámico,
//! bajas diferidas, entorno multi-capa raster, métricas espaciales).

pub mod model;
pub mod optim;
pub mod raster;

pub use model::{
    DebrisAgent, DebrisFlowModel, EnhancedPhysics, Flow, Layers, Metrics, PARAM_DIMS,
    PARAM_DIMS_CHANARAL, ParamDim, Params, Physics, Raindrop, evaluate, evaluate_masked,
    params_chanaral_from_genes, params_from_genes, run_and_score,
};
pub use optim::{Bounds, Method, Outcome};
pub use raster::{CopiapoData, Window, load};
