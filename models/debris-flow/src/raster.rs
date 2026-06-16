//! Carga de rasters raw Float32 (generados por `prepare_data.py`).

use std::fs;
use std::io;
use std::path::Path;

use swarm_core::prelude::*;

use crate::model::Layers;

/// Ventana de evaluación (crop del bbox) en índices de la grilla del DEM.
#[derive(Debug, Clone, Copy)]
pub struct Window {
    pub row_start: usize,
    pub row_end: usize,
    pub col_start: usize,
    pub col_end: usize,
}

/// Stack completo: capas de entrada + ground truth + ventana, y un bounding
/// box de evaluación opcional (presente en Chañaral, ausente en Copiapó).
pub struct CopiapoData {
    pub layers: Layers,
    pub ground_truth: Grid2D<f32>,
    /// Máscara del dominio de evaluación (Chañaral); `None` ⇒ se evalúa toda
    /// la ventana (Copiapó).
    pub bbox: Option<Grid2D<f32>>,
    pub window: Window,
    pub pixel_size: f64,
}

fn load_f32(path: &Path, width: usize, height: usize) -> io::Result<Grid2D<f32>> {
    let bytes = fs::read(path)?;
    let expected = width * height * 4;
    if bytes.len() != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{}: {} bytes, se esperaban {expected} ({width}x{height} f32)",
                path.display(),
                bytes.len()
            ),
        ));
    }
    let values: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    Ok(Grid2D::from_fn(width, height, |p| {
        values[p.y * width + p.x]
    }))
}

/// Carga el stack desde el directorio generado por `prepare_data.py`.
pub fn load(dir: &Path) -> io::Result<CopiapoData> {
    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("meta.json"))?)?;
    let width = meta["width"].as_u64().expect("meta width") as usize;
    let height = meta["height"].as_u64().expect("meta height") as usize;
    let pixel_size = meta["pixel_size"].as_f64().expect("meta pixel_size");
    let win = &meta["window"];
    let window = Window {
        row_start: win["row_start"].as_u64().expect("window") as usize,
        row_end: win["row_end"].as_u64().expect("window") as usize,
        col_start: win["col_start"].as_u64().expect("window") as usize,
        col_end: win["col_end"].as_u64().expect("window") as usize,
    };

    let g = |name: &str| load_f32(&dir.join(format!("{name}.f32")), width, height);
    let bbox_path = dir.join("bbox.f32");
    let bbox = if bbox_path.exists() {
        Some(load_f32(&bbox_path, width, height)?)
    } else {
        None
    };
    Ok(CopiapoData {
        layers: Layers {
            dem: g("dem")?,
            slope: g("slope")?,
            rain: vec![g("rain_dia1")?, g("rain_dia2")?, g("rain_dia3")?],
            isotherm: g("isotherm")?,
            sediment: g("sediment")?,
            susceptibility: g("susceptibility")?,
            streams: g("streams")?,
        },
        ground_truth: g("ground_truth")?,
        bbox,
        window,
        pixel_size,
    })
}
